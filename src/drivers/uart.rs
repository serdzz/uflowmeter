//! USART1 async echo + line-buffer for shell commands.
//!
//! Embassy task that reads bytes from USART1 (PA9 TX / PA10 RX) and
//! both echoes them back AND publishes received lines to a channel
//! (so a shell-command consumer can pick them up). First-pass
//! skeleton — the legacy Modbus RTU framer + shell dispatcher live
//! in src/modbus_handler.rs / src/shell.rs and get ported later.
//!
//! RS485 transceiver power (PC9 = RsPowerEn, active-LOW per the legacy
//! firmware) is enabled in main.rs.

use embassy_stm32::mode::Async;
use embassy_stm32::usart::Uart;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::Vec;

/// One complete shell line (terminated by \r or \n, stripped).
pub type ShellLine = Vec<u8, 80>;
pub type ShellChannel = Channel<CriticalSectionRawMutex, ShellLine, 4>;

pub static SHELL_LINES: ShellChannel = Channel::new();

#[embassy_executor::task]
pub async fn uart_task(mut uart: Uart<'static, Async>) {
    let mut buf = [0u8; 1];
    let mut line: ShellLine = Vec::new();
    loop {
        match uart.read(&mut buf).await {
            Ok(()) => {
                let b = buf[0];
                // Echo back.
                let _ = uart.write(&buf).await;

                if b == b'\r' || b == b'\n' {
                    if !line.is_empty() {
                        defmt::info!("uart line: {=[u8]:a}", line.as_slice());
                        let _ = SHELL_LINES.try_send(line.clone());
                        line.clear();
                    }
                } else if line.push(b).is_err() {
                    // Line too long — drop and start fresh.
                    line.clear();
                    let _ = uart.write(b"!OVERFLOW\r\n").await;
                }
            }
            Err(e) => {
                defmt::error!("uart read error: {}", e);
            }
        }
    }
}
