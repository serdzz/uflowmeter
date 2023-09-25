#![allow(unsafe_code)]

#[macro_export]
macro_rules! SaveLoadGpioReg {
    (
        $GPIOX:ident, $gpiox:ident
    ) => {
        pub mod $gpiox {
            use super::GpioReg;
            use stm32l1xx_hal::stm32::$GPIOX;

            pub fn save() -> GpioReg {
                let reg_base = unsafe { &(*$GPIOX::ptr()) };
                GpioReg {
                    moder: reg_base.moder.read().bits(),
                    otyper: reg_base.otyper.read().bits(),
                    ospeeder: reg_base.ospeedr.read().bits(),
                    afrh: reg_base.afrh.read().bits(),
                    afrl: reg_base.afrl.read().bits(),
                    pupdr: reg_base.pupdr.read().bits(),
                    odr: reg_base.odr.read().bits(),
                }
            }

            pub fn load(val: &GpioReg) {
                let reg_base = unsafe { &(*$GPIOX::ptr()) };
                reg_base.moder.write(|w| unsafe { w.bits(val.moder) });
                reg_base.otyper.write(|w| unsafe { w.bits(val.otyper) });
                reg_base.ospeedr.write(|w| unsafe { w.bits(val.ospeeder) });
                reg_base.afrh.write(|w| unsafe { w.bits(val.afrh) });
                reg_base.afrl.write(|w| unsafe { w.bits(val.afrl) });
                reg_base.pupdr.write(|w| unsafe { w.bits(val.pupdr) });
                reg_base.odr.write(|w| unsafe { w.bits(val.odr) });
            }
        }
    };
}

#[derive(Default, Debug)]
pub struct GpioReg {
    pub moder: u32,
    pub otyper: u32,
    pub ospeeder: u32,
    pub afrh: u32,
    pub afrl: u32,
    pub pupdr: u32,
    pub odr: u32,
}

pub struct GpioPower {
    low_power_state: [GpioReg; 5],
    save_power_state: [GpioReg; 5],
}

impl GpioPower {
    pub fn new() -> Self {
        Self {
            low_power_state: [
                gpioa::save(),
                gpiob::save(),
                gpioc::save(),
                gpiod::save(),
                gpioh::save(),
            ],
            save_power_state: [
                GpioReg::default(),
                GpioReg::default(),
                GpioReg::default(),
                GpioReg::default(),
                GpioReg::default(),
            ],
        }
    }

    pub fn down(&mut self) {
        self.save_power_state[0] = gpioa::save();
        self.save_power_state[1] = gpiob::save();
        self.save_power_state[2] = gpioc::save();
        self.save_power_state[3] = gpiod::save();
        self.save_power_state[4] = gpioh::save();
        gpioa::load(&self.low_power_state[0]);
        gpiob::load(&self.low_power_state[1]);
        gpioc::load(&self.low_power_state[2]);
        gpiod::load(&self.low_power_state[3]);
        gpioh::load(&self.low_power_state[4]);
        // let rcc = unsafe { &(*RCC::ptr()) };
        // rcc.ahbenr.modify(|_, w| w
        //     .gpiopaen().clear_bit()
        //     .gpiopben().clear_bit()
        //     .gpiopcen().clear_bit()
        //     .gpiopden().clear_bit()
        //     .gpiophen().clear_bit()
        // );
    }

    pub fn up(&self) {
        gpioa::load(&self.save_power_state[0]);
        gpiob::load(&self.save_power_state[1]);
        gpioc::load(&self.save_power_state[2]);
        gpiod::load(&self.save_power_state[3]);
        gpioh::load(&self.save_power_state[4]);
        // let rcc = unsafe { &(*RCC::ptr()) };
        // rcc.ahbenr.modify(|_, w| w
        //     .gpiopaen().set_bit()
        //     .gpiopben().set_bit()
        //     .gpiopcen().set_bit()
        //     .gpiopden().set_bit()
        //     .gpiophen().set_bit()
        // );
    }
}

impl Default for GpioPower {
    fn default() -> Self {
        Self::new()
    }
}

SaveLoadGpioReg!(GPIOA, gpioa);
SaveLoadGpioReg!(GPIOB, gpiob);
SaveLoadGpioReg!(GPIOC, gpioc);
SaveLoadGpioReg!(GPIOD, gpiod);
SaveLoadGpioReg!(GPIOH, gpioh);
