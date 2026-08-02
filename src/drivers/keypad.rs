//! 4-button keypad sampled from a dedicated embassy task.
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
//!
//! See `keypad_task` for why presses are detected by sampling rather
//! than by awaiting the EXTI edges.

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

/// Sampling interval while a key is down or was just released.
const ACTIVE_POLL: embassy_time::Duration = embassy_time::Duration::from_millis(20);

/// Sampling interval while idle. Deliberately longer than
/// `Config::min_stop_pause` (100 ms) so the executor can still drop into
/// STOP between samples — a shorter tick here would pin the meter awake.
const IDLE_POLL: embassy_time::Duration = embassy_time::Duration::from_millis(150);

/// How long every key must stay released before going back to idle
/// sampling. Covers the gap between two presses in a burst, so a user
/// working through a menu is served by the fast poll throughout.
const QUIET: embassy_time::Duration = embassy_time::Duration::from_millis(400);

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

/// Embassy task: samples the keypad, using EXTI only to wake up.
///
/// The edges themselves are **not** treated as presses. An earlier
/// version did exactly that — `select4` over the four
/// `wait_for_falling_edge()` futures, emitting a press per edge — and
/// on hardware it lost roughly nineteen presses in twenty. Measured
/// with a minimal example driving nothing but these four pins: the
/// edge-driven build caught about one press in twenty, while sampling
/// the same pins every 20 ms caught 286 of 286, with the levels showing
/// clean transitions in both directions. So the pins and the pull-ups
/// are fine; awaiting the edges is what dropped them. The C++ reaches
/// the same arrangement from the other direction — its `Keyboard::read`
/// has always polled.
///
/// Two sampling rates keep that from costing power:
///
///   * idle — wait for either an EXTI edge or `IDLE_POLL`, whichever
///     comes first. The edge is what wakes the MCU out of STOP; the
///     timer is the backstop for the edges that go missing. At 150 ms
///     it stays above `min_stop_pause`, so STOP still happens between
///     samples.
///   * active — a key is down, or one was released less than `QUIET`
///     ago: sample every 20 ms so presses, releases and the repeat
///     timing are all caught. Bursts of key presses stay in this mode
///     from start to finish.
#[embassy_executor::task]
pub async fn keypad_task(
    mut btn_config: ExtiInput<'static, Async>,
    mut btn_enter: ExtiInput<'static, Async>,
    mut btn_down: ExtiInput<'static, Async>,
    mut btn_up: ExtiInput<'static, Async>,
) {
    use embassy_futures::select::{select, select4};

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
    let mut last_activity = Instant::from_ticks(0);

    loop {
        let idle = state.iter().all(|s| !s.pressed)
            && Instant::now().saturating_duration_since(last_activity) > QUIET;

        if idle {
            // Race the edges against the backstop timer. Nothing is read
            // from the winner — both simply mean "sample now".
            let _ = select(
                select4(
                    btn_config.wait_for_falling_edge(),
                    btn_enter.wait_for_falling_edge(),
                    btn_down.wait_for_falling_edge(),
                    btn_up.wait_for_falling_edge(),
                ),
                Timer::after(IDLE_POLL),
            )
            .await;
        } else {
            Timer::after(ACTIVE_POLL).await;
        }

        let now = Instant::now();
        let lows = [
            btn_config.is_low(),
            btn_enter.is_low(),
            btn_down.is_low(),
            btn_up.is_low(),
        ];

        for i in 0..4 {
            let is_low = lows[i];
            let st = &mut state[i];
            if is_low && !st.pressed {
                st.pressed = true;
                st.next_repeat = now + REPEAT_DELAY;
                last_activity = now;
                if KEYS.try_send(KeyEvent::Pressed(FLAGS[i])).is_err() {
                    defmt::warn!("keypad: KEYS full, press idx={=usize} dropped", i);
                }
            } else if !is_low && st.pressed {
                st.pressed = false;
                last_activity = now;
            } else if st.pressed && now >= st.next_repeat {
                st.next_repeat = now + REPEAT_INTERVAL;
                last_activity = now;
                if KEYS.try_send(KeyEvent::Pressed(FLAGS[i])).is_err() {
                    defmt::warn!("keypad: KEYS full, repeat idx={=usize} dropped", i);
                }
            }
        }
    }
}
