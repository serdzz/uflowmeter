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

use embassy_futures::select::{Either, Either3, select, select3};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Pull;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{Config as UartConfig, Uart};
use embassy_stm32::{Peri, peripherals};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::Irqs;

const FRAME_GAP: Duration = Duration::from_micros(1750);
const MAX_FRAME: usize = 256;

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
) {
    loop {
        // PHASE 1 — idle: pure ExtiInput on PA10. Waits for the line
        // to fall (start bit) or for a pending TX (something we want
        // to send unprompted, e.g. an unsolicited shell prompt).
        let woken_by_tx = {
            let mut rx_exti = ExtiInput::new(
                rx_pin.reborrow(),
                exti10.reborrow(),
                Pull::Up,
                Irqs,
            );
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
        run_session(&mut uart).await;

        // PHASE 4 — uart dropped at end of loop, releasing USART /
        // pins / DMA. STOP refcount drops, executor sleeps.
    }
}

/// Read/dispatch loop. Returns when the line has been quiet for
/// SESSION_IDLE_TIMEOUT and there is no pending TX/frame work.
async fn run_session(uart: &mut Uart<'_, Async>) {
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
                Either3::First(r) => handle_read(r, &buf, &mut line, &mut frame, uart).await,
                Either3::Second(_) => {
                    // Quiet for SESSION_IDLE_TIMEOUT → tear down USART.
                    defmt::info!("uart: session idle, sleep");
                    return;
                }
                Either3::Third(tx_data) => write_tx(&tx_data, uart).await,
            }
        } else {
            // Mid-frame — race FRAME_GAP for modbus boundary detection.
            match select3(uart.read(&mut buf), Timer::after(FRAME_GAP), UART_TX.receive()).await {
                Either3::First(r) => handle_read(r, &buf, &mut line, &mut frame, uart).await,
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
) {
    match r {
        Ok(()) => {
            let b = buf[0];
            if b == b'\r' || b == b'\n' {
                if !line.is_empty() {
                    defmt::info!("uart line: {=[u8]:a}", line.as_slice());
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
    use uflowmeter::shell::{ShellResult, parse_action, process_line};
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
