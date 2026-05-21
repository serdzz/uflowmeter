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

use embassy_stm32::mode::Async;
use embassy_stm32::usart::Uart;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, with_timeout};
use heapless::Vec;

const FRAME_GAP: Duration = Duration::from_micros(1750);
const MAX_FRAME: usize = 256;

pub type ShellLine = Vec<u8, 80>;
pub type ModbusFrame = Vec<u8, MAX_FRAME>;

pub type ShellChannel = Channel<CriticalSectionRawMutex, ShellLine, 4>;
pub type ModbusChannel = Channel<CriticalSectionRawMutex, ModbusFrame, 4>;

pub static SHELL_LINES: ShellChannel = Channel::new();
pub static MODBUS_FRAMES: ModbusChannel = Channel::new();

#[embassy_executor::task]
pub async fn uart_task(mut uart: Uart<'static, Async>) {
    let mut buf = [0u8; 1];
    let mut line: ShellLine = Vec::new();
    let mut frame: ModbusFrame = Vec::new();
    loop {
        match with_timeout(FRAME_GAP, uart.read(&mut buf)).await {
            Ok(Ok(())) => {
                let b = buf[0];
                let _ = uart.write(&buf).await;

                // Shell-line accumulator.
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

                // Modbus byte accumulator (no boundary trigger here;
                // frame is closed on the next read-timeout).
                if frame.push(b).is_err() {
                    defmt::warn!("modbus frame > {} B, dropped", MAX_FRAME);
                    frame.clear();
                }
            }
            Ok(Err(e)) => {
                defmt::error!("uart read error: {}", e);
                line.clear();
                frame.clear();
            }
            Err(_) => {
                // Inter-byte gap exceeded FRAME_GAP — emit any pending
                // modbus frame. (Shell lines wait for explicit \r/\n.)
                if !frame.is_empty() {
                    defmt::info!("modbus frame: {=[u8]:x}", frame.as_slice());
                    let _ = MODBUS_FRAMES.try_send(frame.clone());
                    frame.clear();
                }
            }
        }
    }
}
