#![allow(dead_code)]
use crate::*;
use alloc::string::String;
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
            defmt::trace!("lcd init");
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
            self.lcd
                .upload_character(0u8, [0x1f, 0x0, 0xe, 0x1, 0xf, 0x11, 0xf, 0x0]);
        }
        ret
    }

    pub fn off(&mut self) {
        defmt::trace!("lcd off");
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

    fn convert_utf8(&mut self, c: char) -> u8 {
        match c {
            '0' => b'O',
            '\0'..='\u{ff}' => c as u8,
            'А' => b'A',
            'Б' => b'\0',
            'В' => b'B',
            'Г' => b'\0',
            'Д' => b'\0',
            'Е' => b'E',
            'Ж' => b'\0',
            'З' => b'3',
            'И' => b'\0',
            'Й' => b'\0',
            'К' => b'K',
            'Л' => b'\0',
            'М' => b'M',
            'Н' => b'H',
            'О' => b'O',
            'П' => b'\0',
            'Р' => b'P',
            'С' => b'C',
            'Т' => b'T',
            'У' => b'\0',
            'Ф' => b'\0',
            'Х' => b'X',
            'Ц' => b'\0',
            'Ч' => b'\0',
            'Ш' => b'\0',
            'Щ' => b'\0',
            'Ъ' => b'\0',
            'Ы' => b'\0',
            'Ь' => b'\0',
            'Э' => b'\0',
            'Ю' => b'\0',
            'Я' => b'\0',
            'а' => b'a',
            'б' => b'\0',
            'в' => b'\0',
            'г' => b'\0',
            'д' => b'\0',
            'е' => b'e',
            'ж' => b'\0',
            'з' => b'\0',
            'и' => b'\0',
            'й' => b'\0',
            'к' => b'k',
            'м' => b'm',
            'н' => b'\0',
            'о' => b'o',
            'п' => b'\0',
            'р' => b'p',
            'с' => b'c',
            'т' => b'\0',
            'у' => b'\0',
            'ф' => b'\0',
            'х' => b'x',
            'ц' => b'\0',
            'ч' => b'\0',
            'ш' => b'\0',
            'щ' => b'\0',
            'ъ' => b'\0',
            'ы' => b'\0',
            'ь' => b'\0',
            'э' => b'\0',
            'ю' => b'\0',
            'я' => b'\0',
            _ => b'\0',
        }
    }
}

impl core::fmt::Write for Lcd {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut val = String::new();
        for c in s.chars().enumerate() {
            let ascii = self.convert_utf8(c.1);
            val.push(ascii as char);
        }
        self.lcd.write_str(val.as_str()).ok();
        Ok(())
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
