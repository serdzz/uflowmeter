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

/// Embassy task: two-mode keypad driver.
///
///   * IDLE — no button currently held. Pure `select4` over the four
///     ExtiInput falling-edge futures. No timer is armed, so this
///     branch doesn't keep the embassy time-driver out of STOP and
///     the MCU sleeps until a physical press wakes EXTI.
///   * PRESSED — at least one button is down. Falls back to the
///     legacy 50 ms POLL_INTERVAL so we can detect release + drive
///     the REPEAT_DELAY / REPEAT_INTERVAL repeat fire. Exits back to
///     IDLE once every state slot reports `pressed == false`.
///
/// Power impact: removes the ~20 Hz wake the constant poll used to
/// cost while the device sat untouched.
#[embassy_executor::task]
pub async fn keypad_task(
    mut btn_config: ExtiInput<'static, Async>,
    mut btn_enter: ExtiInput<'static, Async>,
    mut btn_down: ExtiInput<'static, Async>,
    mut btn_up: ExtiInput<'static, Async>,
) {
    use embassy_futures::select::{select4, Either4};

    const FLAGS: [ButtonFlags; 4] = [
        ButtonFlags::CONFIG,
        ButtonFlags::ENTER,
        ButtonFlags::DOWN,
        ButtonFlags::UP,
    ];
    let mut state = [
        ButtonState::new(),
        ButtonState::new(),
        ButtonState::new(),
        ButtonState::new(),
    ];

    loop {
        // IDLE — wait for any falling edge via EXTI. No timer armed.
        let idx = match select4(
            btn_config.wait_for_falling_edge(),
            btn_enter.wait_for_falling_edge(),
            btn_down.wait_for_falling_edge(),
            btn_up.wait_for_falling_edge(),
        )
        .await
        {
            Either4::First(()) => 0,
            Either4::Second(()) => 1,
            Either4::Third(()) => 2,
            Either4::Fourth(()) => 3,
        };
        let now = Instant::now();
        state[idx].pressed = true;
        state[idx].next_repeat = now + REPEAT_DELAY;
        KEYS.try_send(KeyEvent::Pressed(FLAGS[idx])).ok();

        // PRESSED — keep polling at 20 Hz until all keys go up. This
        // is the only window where we add a sub-100 ms alarm; the
        // user is interacting so STOP duty here is expected to drop.
        loop {
            Timer::after(POLL_INTERVAL).await;
            let now = Instant::now();
            let lows = [
                btn_config.is_low(),
                btn_enter.is_low(),
                btn_down.is_low(),
                btn_up.is_low(),
            ];
            let mut any_pressed = false;
            for i in 0..4 {
                let is_low = lows[i];
                let st = &mut state[i];
                if is_low && !st.pressed {
                    st.pressed = true;
                    st.next_repeat = now + REPEAT_DELAY;
                    KEYS.try_send(KeyEvent::Pressed(FLAGS[i])).ok();
                } else if !is_low && st.pressed {
                    st.pressed = false;
                } else if st.pressed && now >= st.next_repeat {
                    st.next_repeat = now + REPEAT_INTERVAL;
                    KEYS.try_send(KeyEvent::Pressed(FLAGS[i])).ok();
                }
                if st.pressed {
                    any_pressed = true;
                }
            }
            if !any_pressed {
                break;
            }
        }
    }
}
