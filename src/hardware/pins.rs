use super::GpioPower;
use stm32l1xx_hal::gpio::gpioa::*;
use stm32l1xx_hal::gpio::gpiob::*;
use stm32l1xx_hal::gpio::gpioc::*;
use stm32l1xx_hal::gpio::GpioExt;
use stm32l1xx_hal::gpio::{Analog, Floating, Input, Output, PullUp, PushPull, Speed};
use stm32l1xx_hal::stm32::*;

pub type LcdD4 = PA4<Output<PushPull>>;
pub type LcdD5 = PA5<Output<PushPull>>;
pub type LcdD6 = PA6<Output<PushPull>>;
pub type LcdD7 = PA7<Output<PushPull>>;
pub type LcdRs = PC1<Output<PushPull>>;
pub type LcdRw = PC2<Output<PushPull>>;
pub type LcdE = PC3<Output<PushPull>>;
pub type LcdOn = PC0<Output<PushPull>>;
pub type LcdLed = PC5<Output<PushPull>>;

pub type ButtonSet = PB6<Input<PullUp>>;
pub type ButtonEnter = PB7<Input<PullUp>>;
pub type ButtonDown = PB8<Input<PullUp>>;
pub type ButtonUp = PB9<Input<PullUp>>;

pub type SpiSck = PB13<Input<Floating>>;
pub type SpiMiso = PB14<Input<Floating>>;
pub type SpiMosi = PB15<Input<Floating>>;

pub type Mco = PA8<Input<Floating>>;
pub type OscEn = PA11<Output<PushPull>>;

pub type Tx = PA9<Input<Floating>>;
pub type Rx = PA10<Input<Floating>>;
pub type RsPowerEn = PC9<Output<PushPull>>;

pub type MemoryEn = PC10<Output<PushPull>>;
pub type MemoryHold = PC11<Output<PushPull>>;
pub type MemoryWp = PC12<Output<PushPull>>;

pub type Tdc1000Cs = PB11<Output<PushPull>>;
pub type Tdc1000En = PB10<Output<PushPull>>;
pub type Tdc1000Res = PC6<Output<PushPull>>;

pub type Tdc7200Cs = PB12<Output<PushPull>>;
pub type Tdc7200En = PB1<Output<PushPull>>;
pub type Tdc7200Int = PB0<Input<PullUp>>;

pub type SwEn = PB3<Output<PushPull>>;
pub type SwA0 = PB4<Output<PushPull>>;
pub type SwA1 = PB5<Output<PushPull>>;

pub type PhotoR = PC4<Analog>;

pub type ExtIn = PC7<Input<PullUp>>;
pub type ExtOut = PC8<Output<PushPull>>;
// pub type MyStorage = Storage<SharedBus<BusType>, MemoryEn, MemoryWp, MemoryHold>;

pub struct Pins {
    // lcd pins
    pub lcd_rs: LcdRs,
    pub lcd_rw: LcdRw,
    pub lcd_e: LcdE,
    pub lcd_d4: LcdD4,
    pub lcd_d5: LcdD5,
    pub lcd_d6: LcdD6,
    pub lcd_d7: LcdD7,
    pub lcd_on: LcdOn,
    pub lcd_led: LcdLed,

    // button pins
    pub button_set: ButtonSet,
    pub button_enter: ButtonEnter,
    pub button_down: ButtonDown,
    pub button_up: ButtonUp,

    // spi pins
    pub spi_sck: SpiSck,
    pub spi_miso: SpiMiso,
    pub spi_mosi: SpiMosi,

    // mco pin
    pub mco: Mco,
    pub osc_en: OscEn,

    // serial port pins
    pub tx: Tx,
    pub rx: Rx,
    pub rs_power_en: RsPowerEn,

    // 25LC1024 pins
    pub memory_en: MemoryEn,
    pub memory_hold: MemoryHold,
    pub memory_wp: MemoryWp,

    // tdc1000 pins
    pub tdc1000_en: Tdc1000En,
    pub tdc1000_cs: Tdc1000Cs,
    pub tdc1000_res: Tdc1000Res,

    // tdc7200 pins
    pub tdc7200_en: Tdc7200En,
    pub tdc7200_cs: Tdc7200Cs,
    pub tdc7200_int: Tdc7200Int,

    // SW pins
    pub sw_en: SwEn,
    pub sw_a0: SwA0,
    pub sw_a1: SwA1,

    // photo_r pin
    pub photo_r: PhotoR,

    // ext pins
    pub ext_in: ExtIn,
    pub ext_out: ExtOut,

    // gpio power
    pub gpio_power: GpioPower,
}

impl Pins {
    pub fn new(gpioa: GPIOA, gpiob: GPIOB, gpioc: GPIOC, gpiod: GPIOD, gpioh: GPIOH) -> Self {
        let port_a = gpioa.split();
        let port_b = gpiob.split();
        let port_c = gpioc.split();
        let port_d = gpiod.split();
        let port_h = gpioh.split();

        #[cfg(feature = "low_power")]
        {
            let _swd_io = port_a.pa13.into_analog();
            let _swd_clk = port_a.pa14.into_analog();
        }
        let _ = port_a.pa0.into_analog(); // WKUP not connected
        let _ = port_a.pa12.into_analog(); // not connected
        let _ = port_a.pa15.into_analog(); // not connected
        let _ = port_c.pc13.into_analog(); // not connected
        let _ = port_c.pc14.into_analog(); // OSC 32Khz
        let _ = port_c.pc15.into_analog(); // OSC 32Khz
        let _ = port_d.pd2.into_analog(); // not connected
        let _ = port_h.ph0.into_analog(); // OSC 8Mhz
        let _ = port_h.ph1.into_analog(); // OSC 8Mhz

        // not used
        let _ir_sd = port_a.pa1.into_pull_up_input();
        let _ir_tx = port_a.pa2.into_pull_down_input();
        let _ir_rx = port_a.pa3.into_pull_up_input();

        let button_set = port_b.pb6.into_pull_up_input();
        let button_enter = port_b.pb7.into_pull_up_input();
        let button_down = port_b.pb8.into_pull_up_input();
        let button_up = port_b.pb9.into_pull_up_input();

        let lcd_rs = port_c.pc1.into_pull_down_input();
        let lcd_rw = port_c.pc2.into_pull_down_input();
        let lcd_e = port_c.pc3.into_pull_down_input();
        let lcd_d4 = port_a.pa4.into_pull_down_input();
        let lcd_d5 = port_a.pa5.into_pull_down_input();
        let lcd_d6 = port_a.pa6.into_pull_down_input();
        let lcd_d7 = port_a.pa7.into_pull_down_input();
        let lcd_on = port_c.pc0.into_pull_up_input();
        let lcd_led = port_c.pc5.into_pull_up_input();

        let mco = port_a.pa8.into_pull_down_input();
        let osc_en = port_a.pa11.into_pull_up_input();

        let spi_sck = port_b.pb13.into_pull_down_input();
        let spi_miso = port_b.pb14.into_pull_down_input();
        let spi_mosi = port_b.pb15.into_pull_down_input();

        let tdc7200_int = port_b.pb0.into_pull_down_input();
        let tdc7200_en = port_b.pb1.into_pull_down_input();
        let tdc7200_cs = port_b.pb12.into_pull_up_input();

        let tdc1000_en = port_b.pb10.into_pull_down_input();
        let tdc1000_cs = port_b.pb11.into_pull_up_input();
        let tdc1000_res = port_c.pc6.into_pull_down_input();

        let sw_en = port_b.pb3.into_pull_down_input();
        let sw_a0 = port_b.pb4.into_pull_down_input();
        let sw_a1 = port_b.pb5.into_pull_down_input();

        let photo_r = port_c.pc4.into_pull_down_input();

        let ext_in = port_c.pc7.into_pull_up_input();
        let ext_out = port_c.pc8.into_pull_down_input();

        let tx = port_a.pa9.into_pull_down_input();
        let rx = port_a.pa10.into_pull_down_input();
        let rs_power_en = port_c.pc9.into_pull_up_input();

        let memory_en = port_c.pc10.into_pull_up_input();
        let memory_hold = port_c.pc11.into_pull_down_input();
        let memory_wp = port_c.pc12.into_pull_down_input();

        let gpio_power = GpioPower::new();

        Self {
            // lcd pins
            lcd_rs: lcd_rs.into_push_pull_output(),
            lcd_rw: lcd_rw.into_push_pull_output(),
            lcd_e: lcd_e.into_push_pull_output(),
            lcd_d4: lcd_d4.into_push_pull_output(),
            lcd_d5: lcd_d5.into_push_pull_output(),
            lcd_d6: lcd_d6.into_push_pull_output(),
            lcd_d7: lcd_d7.into_push_pull_output(),
            lcd_on: lcd_on.into_push_pull_output(),
            lcd_led: lcd_led.into_push_pull_output(),
            // button pins
            button_set: button_set.into_pull_up_input(),
            button_enter: button_enter.into_pull_up_input(),
            button_down: button_down.into_pull_up_input(),
            button_up: button_up.into_pull_up_input(),
            // spi pins
            spi_sck: spi_sck.into_floating_input().set_speed(Speed::VeryHigh),
            spi_miso: spi_miso.into_floating_input().set_speed(Speed::VeryHigh),
            spi_mosi: spi_mosi.into_floating_input().set_speed(Speed::VeryHigh),
            // mco pin
            mco: mco.into_floating_input().set_speed(Speed::VeryHigh),
            osc_en: osc_en.into_push_pull_output(),
            // serial port pins
            tx: tx.into_floating_input(),
            rx: rx.into_floating_input(),
            rs_power_en: rs_power_en.into_push_pull_output(),
            // 25LC1024 pins
            memory_en: memory_en.into_push_pull_output(),
            memory_hold: memory_hold.into_push_pull_output(),
            memory_wp: memory_wp.into_push_pull_output(),
            // tdc1000 pins
            tdc1000_en: tdc1000_en.into_push_pull_output(),
            tdc1000_cs: tdc1000_cs.into_push_pull_output(),
            tdc1000_res: tdc1000_res.into_push_pull_output(),
            // tdc7200 pins
            tdc7200_en: tdc7200_en.into_push_pull_output(),
            tdc7200_cs: tdc7200_cs.into_push_pull_output(),
            tdc7200_int: tdc7200_int.into_pull_up_input(),
            // SW pins
            sw_en: sw_en.into_push_pull_output(),
            sw_a0: sw_a0.into_push_pull_output(),
            sw_a1: sw_a1.into_push_pull_output(),
            // photo_r pin
            photo_r: photo_r.into_analog(),
            // ext pins
            ext_in: ext_in.into_pull_up_input(),
            ext_out: ext_out.into_push_pull_output(),
            // gpio power
            gpio_power,
        }
    }
}
