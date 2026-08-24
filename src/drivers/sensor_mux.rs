//! Transducer-pair multiplexer on PB3 / PB4 / PB5.
//!
//! Port of the C++ `Hardware::SensorMux`
//! (`UFlowMeter_c++/UFlowMeter/hardware/sensor_mux.cpp`), which drives
//! the three pins as one 3-bit `PinList<Pb3, Pb4, Pb5>` — PB3 is the
//! low bit — and writes these values:
//!
//! | channel | value | PB3 | PB4 | PB5 |
//! |---------|-------|-----|-----|-----|
//! | Off     | 0     |  0  |  0  |  0  |
//! | One     | 1     |  1  |  0  |  0  |
//! | Two     | 3     |  1  |  1  |  0  |
//!
//! So PB3 is the mux enable, PB4 selects the pair, and PB5 is unused
//! at present — it is driven low to keep the third address line from
//! floating next to the analog frontend.

use embassy_stm32::gpio::Output;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Off = 0,
    One = 1,
    Two = 3,
}

impl Channel {
    /// The sensor pair for index 0 or 1; anything else is `Off`.
    pub fn for_sensor(index: usize) -> Self {
        match index {
            0 => Channel::One,
            1 => Channel::Two,
            _ => Channel::Off,
        }
    }
}

pub struct SensorMux<'d> {
    en: Output<'d>,
    a0: Output<'d>,
    a1: Output<'d>,
}

impl<'d> SensorMux<'d> {
    /// `en` is PB3, `a0` is PB4, `a1` is PB5. Starts in `Off`, which
    /// is what the C++ `init()` writes.
    pub fn new(en: Output<'d>, a0: Output<'d>, a1: Output<'d>) -> Self {
        let mut mux = Self { en, a0, a1 };
        mux.set_channel(Channel::Off);
        mux
    }

    pub fn set_channel(&mut self, ch: Channel) {
        let bits = ch as u8;
        set_bit(&mut self.en, bits & 0x01);
        set_bit(&mut self.a0, bits & 0x02);
        set_bit(&mut self.a1, bits & 0x04);
    }
}

fn set_bit(pin: &mut Output<'_>, value: u8) {
    if value != 0 {
        pin.set_high();
    } else {
        pin.set_low();
    }
}
