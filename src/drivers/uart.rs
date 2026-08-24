//! USART1 async byte reader + Modbus RTU framer.
//!
//! Two output channels:
//!   * `SHELL_LINES` — \r/\n-terminated ASCII lines
//!   * `MODBUS_FRAMES` — binary frames bounded by 3.5-char-time silence
//!
//! Discrimination is heuristic: bytes are accumulated into both
//! buffers; a frame is emitted whenever the corresponding boundary
//! fires. If the first byte of a transmission is printable ASCII the
//! line buffer wins; otherwise the modbus buffer wins. Downstream
//! consumers can pick whichever channel they care about.
//!
//! Modbus RTU spec: at baudrates above 19200 the inter-frame silence
//! is fixed at 1.75 ms (3.5 char times at 19200). At 115200 the real
//! 3.5 char time is ~305 µs but the spec floor wins, so we use 1.75 ms.

use embassy_futures::select::{select, select3, Either, Either3};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::flash::{Blocking, Flash};
use embassy_stm32::gpio::Pull;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{Config as UartConfig, Uart};
use embassy_stm32::{peripherals, Peri};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant, Timer};
use heapless::Vec;

use crate::drivers::slot_b::SlotBWriter;
use crate::Irqs;
use uflowmeter::shell::ShellAction;
use uflowmeter::upload_lib::{self, UploadKind};
use uflowmeter::xmodem_lib::{self, Response, XModemReceiver};

/// Cancel byte, echoed twice when the sender cancels.
const CAN: u8 = 0x18;

const FRAME_GAP: Duration = Duration::from_micros(1750);
const MAX_FRAME: usize = 256;

/// How often to re-send 'C' while waiting for the sender to start.
const XMODEM_POLL: Duration = Duration::from_millis(500);
/// How long a transfer may stall before it is abandoned. Applied
/// between events rather than across the whole transfer, so it serves a
/// 56-byte blob and a 120 KiB firmware image equally — see `pump`.
const XMODEM_STALL: Duration = Duration::from_secs(15);

/// What a recognised command line asks the session to do inline,
/// rather than handing to `shell_task`. Both need the line to
/// themselves from the prompt onwards, so neither can go through the
/// channel.
enum SessionRequest {
    /// A fixed-size configuration blob, applied via the main loop.
    Config(UploadKind),
    /// A firmware image, staged into flash slot B. Does not return.
    Firmware,
}

pub type ShellLine = Vec<u8, 80>;
pub type ModbusFrame = Vec<u8, MAX_FRAME>;

pub type ShellChannel = Channel<CriticalSectionRawMutex, ShellLine, 4>;
pub type ModbusChannel = Channel<CriticalSectionRawMutex, ModbusFrame, 4>;

pub static SHELL_LINES: ShellChannel = Channel::new();
pub static MODBUS_FRAMES: ModbusChannel = Channel::new();
/// Outbound queue for any task that wants to write to USART1 — the
/// Modbus dispatcher pushes its response frames here. uart_task drains
/// them between RX cycles.
pub static UART_TX: ModbusChannel = Channel::new();
/// Shell side-effects parsed out of incoming command lines. shell_task
/// pushes here whenever `shell::parse_action` returns Some; the main
/// loop drains them via select5 and applies (set RTC, mutate Options,
/// etc.) since those resources live in main loop's scope.
pub static SHELL_ACTIONS: Channel<CriticalSectionRawMutex, uflowmeter::shell::ShellAction, 4> =
    Channel::new();

/// Time the USART stays initialised after the last RX/TX event
/// before we tear it down again. Long enough for a Modbus master to
/// retry once after its first attempt loses the wake-byte; short
/// enough to spend most of the time in STOP.
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_millis(800);

/// EXTI-wake UART session task. While idle, PA10 is an EXTI input
/// pulled high — that doesn't bump REFCOUNT_STOP1, so the embassy
/// low-power layer is free to enter STOP. The first incoming start
/// bit (PA10 falls) wakes us; we then initialise the USART proper
/// and run a normal framer session until the line stays quiet for
/// `SESSION_IDLE_TIMEOUT`, at which point we tear down USART and
/// drop back to the EXTI wait.
///
/// Wake-byte loss: the first byte of every new transmission is lost
/// (USART is still off when the start bit arrives). Standard Modbus
/// masters retry on CRC mismatch, so this is acceptable. Shell users
/// will lose the first character of the first command after idle;
/// terminal echo helps the operator notice.
#[embassy_executor::task]
pub async fn uart_session_task(
    mut usart_peri: Peri<'static, peripherals::USART1>,
    mut rx_pin: Peri<'static, peripherals::PA10>,
    mut tx_pin: Peri<'static, peripherals::PA9>,
    mut tx_dma: Peri<'static, peripherals::DMA1_CH4>,
    mut rx_dma: Peri<'static, peripherals::DMA1_CH5>,
    mut exti10: Peri<'static, peripherals::EXTI10>,
    flash_peri: Peri<'static, peripherals::FLASH>,
) {
    // Held for the life of the task rather than acquired per update:
    // the peripheral is a singleton, and the update path must not be
    // able to fail because something else took it first.
    let mut flash = Flash::new_blocking(flash_peri);

    loop {
        // PHASE 1 — idle: pure ExtiInput on PA10. Waits for the line
        // to fall (start bit) or for a pending TX (something we want
        // to send unprompted, e.g. an unsolicited shell prompt).
        let woken_by_tx = {
            let mut rx_exti = ExtiInput::new(rx_pin.reborrow(), exti10.reborrow(), Pull::Up, Irqs);
            match select(rx_exti.wait_for_falling_edge(), UART_TX.receive()).await {
                Either::First(()) => {
                    defmt::info!("uart: wake (EXTI on PA10)");
                    None
                }
                Either::Second(tx) => {
                    defmt::info!("uart: wake (UART_TX pending, {} B)", tx.len());
                    Some(tx)
                }
            }
        }; // ExtiInput drops here → PA10 / EXTI10 free again

        // PHASE 2 — init USART for a session.
        let mut cfg = UartConfig::default();
        cfg.baudrate = 115200;
        let mut uart = match Uart::new(
            usart_peri.reborrow(),
            rx_pin.reborrow(),
            tx_pin.reborrow(),
            tx_dma.reborrow(),
            rx_dma.reborrow(),
            Irqs,
            cfg,
        ) {
            Ok(u) => u,
            Err(e) => {
                defmt::error!("uart: init failed {:?}", defmt::Debug2Format(&e));
                continue;
            }
        };

        // If we were woken by TX, flush it first.
        if let Some(tx) = woken_by_tx {
            if let Err(e) = uart.write(&tx).await {
                defmt::error!("uart write error: {}", e);
            }
        }

        // PHASE 3 — run the framer session until the line goes quiet.
        run_session(&mut uart, &mut flash).await;

        // PHASE 4 — uart dropped at end of loop, releasing USART /
        // pins / DMA. STOP refcount drops, executor sleeps.
    }
}

/// Read/dispatch loop. Returns when the line has been quiet for
/// SESSION_IDLE_TIMEOUT and there is no pending TX/frame work.
async fn run_session(uart: &mut Uart<'_, Async>, flash: &mut Flash<'static, Blocking>) {
    let mut buf = [0u8; 1];
    let mut line: ShellLine = Vec::new();
    let mut frame: ModbusFrame = Vec::new();

    loop {
        if frame.is_empty() && line.is_empty() {
            // No frame in progress — wait for RX, TX, or session timeout.
            match select3(
                uart.read(&mut buf),
                Timer::after(SESSION_IDLE_TIMEOUT),
                UART_TX.receive(),
            )
            .await
            {
                Either3::First(r) => {
                    if let Some(req) = handle_read(r, &buf, &mut line, &mut frame, uart).await {
                        match req {
                            SessionRequest::Config(kind) => run_xmodem(uart, kind).await,
                            SessionRequest::Firmware => run_firmware_update(uart, flash).await,
                        }
                    }
                }
                Either3::Second(_) => {
                    // Quiet for SESSION_IDLE_TIMEOUT → tear down USART.
                    defmt::info!("uart: session idle, sleep");
                    return;
                }
                Either3::Third(tx_data) => write_tx(&tx_data, uart).await,
            }
        } else {
            // Mid-frame — race FRAME_GAP for modbus boundary detection.
            match select3(
                uart.read(&mut buf),
                Timer::after(FRAME_GAP),
                UART_TX.receive(),
            )
            .await
            {
                Either3::First(r) => {
                    if let Some(req) = handle_read(r, &buf, &mut line, &mut frame, uart).await {
                        match req {
                            SessionRequest::Config(kind) => run_xmodem(uart, kind).await,
                            SessionRequest::Firmware => run_firmware_update(uart, flash).await,
                        }
                    }
                }
                Either3::Second(_) => {
                    if !frame.is_empty() {
                        defmt::info!("modbus frame: {=[u8]:x}", frame.as_slice());
                        let _ = MODBUS_FRAMES.try_send(frame.clone());
                        frame.clear();
                    }
                }
                Either3::Third(tx_data) => write_tx(&tx_data, uart).await,
            }
        }
    }
}

async fn handle_read(
    r: Result<(), embassy_stm32::usart::Error>,
    buf: &[u8; 1],
    line: &mut ShellLine,
    frame: &mut ModbusFrame,
    uart: &mut Uart<'_, Async>,
) -> Option<SessionRequest> {
    match r {
        Ok(()) => {
            let b = buf[0];
            if b == b'\r' || b == b'\n' {
                if !line.is_empty() {
                    defmt::info!("uart line: {=[u8]:a}", line.as_slice());
                    // An upload command is handled by the session
                    // itself rather than shell_task: the transfer must
                    // start immediately after the prompt, with no other
                    // writer touching the line in between.
                    match uflowmeter::shell::parse_action(line) {
                        Some(ShellAction::Upload(kind)) => {
                            line.clear();
                            frame.clear();
                            return Some(SessionRequest::Config(kind));
                        }
                        Some(ShellAction::FirmwareUpdate) => {
                            line.clear();
                            frame.clear();
                            return Some(SessionRequest::Firmware);
                        }
                        _ => {}
                    }
                    let _ = SHELL_LINES.try_send(line.clone());
                    line.clear();
                }
            } else if line.push(b).is_err() {
                line.clear();
                let _ = uart.write(b"!OVERFLOW\r\n").await;
            }
            if frame.push(b).is_err() {
                defmt::warn!("modbus frame > {} B, dropped", MAX_FRAME);
                frame.clear();
            }
        }
        Err(e) => {
            defmt::error!("uart read error: {}", e);
            line.clear();
            frame.clear();
        }
    }
    None
}

async fn write_tx(tx_data: &ModbusFrame, uart: &mut Uart<'_, Async>) {
    defmt::info!("uart tx {} B", tx_data.len());
    if let Err(e) = uart.write(tx_data).await {
        defmt::error!("uart write error: {}", e);
    }
}

/// Shell dispatcher: pulls lines from SHELL_LINES, runs them through
/// `uflowmeter::shell::process_line`, writes the response back via
/// UART_TX. NotAShellCommand lines are silently dropped — they'll
/// have already been picked up by the Modbus framer on the same
/// USART.
#[embassy_executor::task]
pub async fn shell_task() {
    use uflowmeter::shell::{parse_action, process_line, ShellResult};
    loop {
        let line = SHELL_LINES.receive().await;
        // Side-effects first so the action is queued even if the
        // reply write blocks momentarily.
        if let Some(action) = parse_action(&line) {
            if SHELL_ACTIONS.try_send(action).is_err() {
                defmt::warn!("shell: SHELL_ACTIONS full, action dropped");
            }
        }
        match process_line(&line) {
            ShellResult::Ok(reply) => {
                let mut buf: ModbusFrame = Vec::new();
                if buf.extend_from_slice(reply.as_bytes()).is_ok() {
                    if UART_TX.try_send(buf).is_err() {
                        defmt::warn!("shell: UART_TX full, reply dropped");
                    }
                } else {
                    defmt::warn!("shell: reply > {} B, dropped", MAX_FRAME);
                }
            }
            ShellResult::Error(msg) => {
                let mut buf: ModbusFrame = Vec::new();
                let _ = buf.extend_from_slice(b"ERR: ");
                let _ = buf.extend_from_slice(msg.as_bytes());
                let _ = buf.extend_from_slice(b"\r\n");
                let _ = UART_TX.try_send(buf);
            }
            ShellResult::NotAShellCommand => {
                // Likely Modbus or noise — drop quietly.
            }
        }
    }
}

/// Receive one configuration blob over XMODEM-CRC and hand it to the
/// main loop as a `ShellAction::ApplyUpload`.
///
/// Runs inline in the session so nothing else writes to the line while
/// the transfer is in flight. The protocol itself lives in
/// `uflowmeter::xmodem_lib`; this is only the transport: poll with 'C'
/// until the sender starts, feed bytes in, send back what the state
/// machine asks for, and give up after XMODEM_TIMEOUT.
async fn run_xmodem(uart: &mut Uart<'_, Async>, kind: UploadKind) {
    let prompt: &[u8] = match kind {
        UploadKind::Calibration => b"Upload 56 bytes (14 f32) with XMODEM-CRC.\r\n",
        UploadKind::TdcRegs => b"Upload 20 bytes (10 TDC1000 + 10 TDC7200) with XMODEM-CRC.\r\n",
    };
    let _ = uart.write(prompt).await;

    let mut data = [0u8; upload_lib::CALIBRATION_BLOCK];

    // Scoped so the receiver's borrow of `data` ends before we read it.
    let (complete, written) = {
        let mut rx = XModemReceiver::new(xmodem_lib::SliceSink::new(&mut data[..kind.block_len()]));
        pump(uart, &mut rx).await;
        (rx.is_complete(), rx.written())
    };

    if complete && written == kind.block_len() {
        let _ = uart.write(b"\r\nDone\r\n").await;
        let _ = SHELL_ACTIONS.try_send(ShellAction::ApplyUpload(kind, data));
    } else {
        defmt::warn!(
            "xmodem: transfer failed (complete={}, {=usize}/{=usize} B)",
            complete,
            written,
            kind.block_len()
        );
        let _ = uart.write(b"\r\nUpload error\r\n").await;
    }
}

/// Drive an XMODEM transfer to completion: poll with 'C' until the
/// sender starts, feed received bytes into the state machine, and send
/// back whatever it asks for.
///
/// The timeout is a **stall** timeout, not a deadline for the whole
/// transfer. A 56-byte configuration blob and a 120 KiB firmware image
/// differ by three orders of magnitude in how long they legitimately
/// take — at this line rate the image is minutes — so an overall
/// deadline would either cut long transfers short or let a dead link
/// hang for minutes. What both have in common is that a healthy sender
/// never goes quiet for `XMODEM_STALL`. Polls do not count as progress,
/// so a sender that never starts still times out.
async fn pump<S: xmodem_lib::Sink>(uart: &mut Uart<'_, Async>, rx: &mut XModemReceiver<S>) {
    let mut byte = [0u8; 1];
    let mut last_progress = Instant::now();

    while !rx.is_finished() {
        // While the sender has not started, re-offer CRC mode. Once it
        // has, only the read arm matters — the poll would inject stray
        // bytes into the packet stream.
        let poll = async {
            if rx.is_waiting() {
                Timer::after(XMODEM_POLL).await
            } else {
                core::future::pending::<()>().await
            }
        };

        match select3(
            uart.read(&mut byte),
            poll,
            Timer::at(last_progress + XMODEM_STALL),
        )
        .await
        {
            Either3::First(Ok(())) => {
                last_progress = Instant::now();
                let reply = match rx.feed(byte[0]) {
                    Response::None => None,
                    Response::Ack => Some(&[xmodem_lib::ACK][..]),
                    Response::Nak => Some(&[xmodem_lib::NAK][..]),
                    Response::CancelAck => Some(&[CAN, CAN][..]),
                };
                if let Some(bytes) = reply {
                    let _ = uart.write(bytes).await;
                }
            }
            Either3::First(Err(e)) => {
                defmt::error!("xmodem: read error {}", e);
                return;
            }
            Either3::Second(()) => {
                let _ = uart.write(&[xmodem_lib::POLL]).await;
            }
            Either3::Third(()) => {
                defmt::warn!("xmodem: sender went quiet, giving up");
                return;
            }
        }
    }
}

/// Receive an encrypted firmware image into flash slot B, then reset so
/// the bootloader installs it.
///
/// Nothing is reported back to the main loop, because on success this
/// function does not return — the device reboots into the bootloader,
/// which is where verification and installation happen. The application
/// deliberately does not check the image's signature itself: it has no
/// business holding the key, and a second check here would only be able
/// to disagree with the one that matters.
async fn run_firmware_update(uart: &mut Uart<'_, Async>, flash: &mut Flash<'static, Blocking>) {
    // The erase is ~480 pages and takes long enough to notice, so it
    // happens before the sender is invited to start.
    let _ = uart.write(b"Erasing staging slot...\r\n").await;
    let writer = match SlotBWriter::new(flash) {
        Ok(w) => w,
        Err(e) => {
            defmt::error!("update: could not prepare slot B ({})", e);
            let _ = uart.write(b"ERROR: could not erase staging slot\r\n").await;
            return;
        }
    };

    let _ = uart
        .write(b"Send the .ufw image with XMODEM-CRC now.\r\n")
        .await;

    let mut rx = XModemReceiver::new(writer);
    pump(uart, &mut rx).await;

    if !rx.is_complete() {
        defmt::warn!(
            "update: transfer did not complete ({=usize} B)",
            rx.written()
        );
        let _ = uart.write(b"\r\nERROR: transfer failed\r\n").await;
        return;
    }

    match rx.into_sink().finish() {
        Ok(header) => {
            defmt::info!(
                "update: staged {=u32} bytes, version {=u32}; resetting",
                header.payload_len,
                header.image_version
            );
            let _ = uart.write(b"\r\nStaged. Resetting...\r\n").await;
            // Give the last bytes time to clear the shift register —
            // resetting mid-frame would leave the operator staring at a
            // truncated line and wondering whether it worked.
            Timer::after(embassy_time::Duration::from_millis(100)).await;
            cortex_m::peripheral::SCB::sys_reset()
        }
        Err(e) => {
            defmt::error!("update: staging failed ({})", e);
            let _ = uart.write(b"\r\nERROR: image rejected\r\n").await;
        }
    }
}
