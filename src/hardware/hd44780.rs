use super::pins::{LcdD4, LcdD5, LcdD6, LcdD7, LcdE, LcdRs, LcdRw};
use embedded_hal::digital::v2::OutputPin;

pub struct LcdHardware {
    rs: LcdRs,
    enable: LcdE,
    d4: LcdD4,
    d5: LcdD5,
    d6: LcdD6,
    d7: LcdD7,
    _rw: LcdRw,
}

impl LcdHardware {
    pub fn new(
        rs: LcdRs,
        enable: LcdE,
        d4: LcdD4,
        d5: LcdD5,
        d6: LcdD6,
        d7: LcdD7,
        _rw: LcdRw,
    ) -> Self {
        Self {
            rs,
            enable,
            d4,
            d5,
            d6,
            d7,
            _rw,
        }
    }
}

impl lcd::Hardware for LcdHardware {
    fn rs(self: &mut LcdHardware, bit: bool) {
        self.rs.set_state(bit.into()).unwrap();
    }

    fn enable(self: &mut LcdHardware, bit: bool) {
        self.enable.set_state(bit.into()).unwrap();
    }

    fn data(self: &mut LcdHardware, data: u8) {
        let d4 = (data & 0b0000_0001) != 0;
        let d5 = (data & 0b0000_0010) != 0;
        let d6 = (data & 0b0000_0100) != 0;
        let d7 = (data & 0b0000_1000) != 0;
        self.d4.set_state(d4.into()).unwrap();
        self.d5.set_state(d5.into()).unwrap();
        self.d6.set_state(d6.into()).unwrap();
        self.d7.set_state(d7.into()).unwrap();
    }
}

impl lcd::Delay for LcdHardware {
    fn delay_us(self: &mut LcdHardware, delay_usec: u32) {
        for _ in 1..=delay_usec {
            cortex_m::asm::delay(32);
        }
    }
}
