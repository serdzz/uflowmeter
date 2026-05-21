//! 4-button keypad polled at 20 Hz from a dedicated embassy task.
//!
//! Pin map per `src/hardware/pins.rs`:
//!   PB6 — Set / Config
//!   PB7 — Enter
//!   PB8 — Down
//!   PB9 — Up
//!
//! All four are pull-up inputs; pressed reads LOW. The task emits
//! `KeyEvent::Pressed(flags)` once per press with a 1 s initial repeat
//! delay then a 150 ms repeat interval — matches the legacy
//! `Keyboard::read` behavior in src/hardware/keyboard.rs.

use bitflags::bitflags;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::mode::Async;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct ButtonFlags: u8 {
        const CONFIG = 0b0001;
        const ENTER  = 0b0010;
        const DOWN   = 0b0100;
        const UP     = 0b1000;
    }
}

/// What the UI sees. Released events are not emitted — UI doesn't use
/// them (mirror of the legacy keyboard.rs surface).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyEvent {
    Pressed(ButtonFlags),
}

/// SPMC channel: the keypad task is the only producer, UI tasks consume.
pub type KeyChannel = Channel<CriticalSectionRawMutex, KeyEvent, 8>;

/// Static channel exported for binding by the executor.
pub static KEYS: KeyChannel = Channel::new();

const REPEAT_DELAY: embassy_time::Duration = embassy_time::Duration::from_millis(1000);
const REPEAT_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_millis(150);
const POLL_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_millis(50);

struct ButtonState {
    pressed: bool,
    next_repeat: Instant,
}

impl ButtonState {
    const fn new() -> Self {
        Self {
            pressed: false,
            next_repeat: Instant::from_ticks(0),
        }
    }
}

/// Embassy task: poll the four button pins, emit press / repeat events
/// into the global KEYS channel. Pins are `ExtiInput` rather than plain
/// `Input` so the EXTI line stays armed and a button press wakes the
/// MCU from STOP mode (see drivers/lowpower.rs).
#[embassy_executor::task]
pub async fn keypad_task(
    btn_config: ExtiInput<'static, Async>,
    btn_enter: ExtiInput<'static, Async>,
    btn_down: ExtiInput<'static, Async>,
    btn_up: ExtiInput<'static, Async>,
) {
    let pins: [(ExtiInput<'static, Async>, ButtonFlags); 4] = [
        (btn_config, ButtonFlags::CONFIG),
        (btn_enter, ButtonFlags::ENTER),
        (btn_down, ButtonFlags::DOWN),
        (btn_up, ButtonFlags::UP),
    ];
    let mut state = [ButtonState::new(), ButtonState::new(), ButtonState::new(), ButtonState::new()];

    loop {
        let now = Instant::now();
        for (idx, (pin, flag)) in pins.iter().enumerate() {
            let is_low = pin.is_low();
            let st = &mut state[idx];
            if is_low && !st.pressed {
                st.pressed = true;
                st.next_repeat = now + REPEAT_DELAY;
                KEYS.try_send(KeyEvent::Pressed(*flag)).ok();
            } else if !is_low && st.pressed {
                st.pressed = false;
            } else if st.pressed && now >= st.next_repeat {
                st.next_repeat = now + REPEAT_INTERVAL;
                KEYS.try_send(KeyEvent::Pressed(*flag)).ok();
            }
        }
        Timer::after(POLL_INTERVAL).await;
    }
}
