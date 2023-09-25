use crate::*;
use lcd::*;

pub struct Lcd {
    lcd: Display<LcdHardware>,
    on: LcdOn,
    led: LcdLed,
    not_active: bool,
}

impl Lcd {
    pub fn new(hd44780: LcdHardware, on: LcdOn, led: LcdLed) -> Self {
        Self {
            lcd: Display::new(hd44780),
            on,
            led,
            not_active: true,
        }
    }

    pub fn init(&mut self) -> bool {
        let ret = self.not_active;
        if self.not_active {
            self.not_active = false;
            self.led.set_high().unwrap();
            self.on.set_low().unwrap();
            self.lcd.init(FunctionLine::Line2, FunctionDots::Dots5x8);
            self.lcd.display(
                DisplayMode::DisplayOn,
                DisplayCursor::CursorOff,
                DisplayBlink::BlinkOff,
            );
            self.lcd
                .entry_mode(EntryModeDirection::EntryRight, EntryModeShift::NoShift);
        }
        ret
    }

    pub fn off(&mut self) {
        self.on.set_high().unwrap();
        self.not_active = true;
    }

    pub fn led(&mut self, on: bool) {
        if on {
            self.led_on();
        } else {
            self.led_off();
        }
    }

    pub fn led_on(&mut self) {
        self.led.set_low().unwrap();
    }

    pub fn led_off(&mut self) {
        self.led.set_high().unwrap();
    }
}

impl core::fmt::Write for Lcd {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.lcd.write_str(s)
    }
}

impl CharacterDisplay for Lcd {
    fn set_position(&mut self, col: u8, row: u8) {
        self.lcd.position(col, row);
    }

    fn clear(&mut self) {
        self.lcd.clear();
    }
}
