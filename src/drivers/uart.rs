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
use embassy_stm32::mode::Async;
use embassy_stm32::usart::Uart;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::Vec;

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

#[embassy_executor::task]
pub async fn uart_task(mut uart: Uart<'static, Async>) {
    let mut buf = [0u8; 1];
    let mut line: ShellLine = Vec::new();
    let mut frame: ModbusFrame = Vec::new();
    loop {
        if frame.is_empty() && line.is_empty() {
            // Idle: pure async wait on RX or TX — no timers, so the
            // executor's low-power layer is free to enter STOP.
            match select(uart.read(&mut buf), UART_TX.receive()).await {
                Either::First(r) => handle_read(r, &buf, &mut line, &mut frame, &mut uart).await,
                Either::Second(tx_data) => write_tx(&tx_data, &mut uart).await,
            }
        } else {
            // Have pending bytes — race RX against the Modbus inter-byte
            // gap timer + pending TX. Only this branch keeps a sub-10ms
            // alarm armed, which is fine because the master is actively
            // sending us a frame.
            match select3(uart.read(&mut buf), Timer::after(FRAME_GAP), UART_TX.receive()).await {
                Either3::First(r) => handle_read(r, &buf, &mut line, &mut frame, &mut uart).await,
                Either3::Second(_) => {
                    // Inter-byte gap exceeded → close out the modbus frame.
                    // (Shell lines wait for explicit \r/\n.)
                    if !frame.is_empty() {
                        defmt::info!("modbus frame: {=[u8]:x}", frame.as_slice());
                        let _ = MODBUS_FRAMES.try_send(frame.clone());
                        frame.clear();
                    }
                }
                Either3::Third(tx_data) => write_tx(&tx_data, &mut uart).await,
            }
        }
    }
}

async fn handle_read(
    r: Result<(), embassy_stm32::usart::Error>,
    buf: &[u8; 1],
    line: &mut ShellLine,
    frame: &mut ModbusFrame,
    uart: &mut Uart<'static, Async>,
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

async fn write_tx(tx_data: &ModbusFrame, uart: &mut Uart<'static, Async>) {
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
