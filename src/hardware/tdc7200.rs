#![allow(dead_code)]

use bitflags::bitflags;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::digital::v2::OutputPin;
use volatile_register::{RO as RORegister, RW as RWRegister}; // Закомментировано, требует добавления зависимости

// Чтобы использовать volatile_register, добавьте следующее в ваш Cargo.toml:
// [dependencies]
// volatile-register = "0.2"
//
// И раскомментируйте строку выше:
// use volatile_register::{RORegister, RWRegister, WORegister};

// Определяем регистры TDC7200
#[repr(C)]
pub struct Tdc7200Registers {
    pub config1: RWRegister<u8>,
    pub config2: RWRegister<u8>,
    pub main_control: RWRegister<u8>,
    pub trigger1_edge: RWRegister<u8>,
    pub trigger2_edge: RWRegister<u8>,
    pub int_mask: RWRegister<u8>,
    pub int_status: RORegister<u8>,
    pub timeout1: RWRegister<u16>,
    pub timeout2: RWRegister<u16>,
    pub clock_count_overflow_err_stat: RORegister<u8>,
    pub num_coarse_conversions: RWRegister<u8>,
    pub measurement1: RORegister<u32>,
    pub measurement2: RORegister<u32>,
    pub coarse_counter_overflow_count1: RORegister<u8>,
    pub coarse_counter_overflow_count2: RORegister<u8>,
    pub reference_clock_counter: RORegister<u32>,
}

// Определяем битовые поля для регистров CONFIG1 и CONFIG2
bitflags! {
    #[repr(transparent)]
    pub struct Config1: u8 {
        const START_MEASUREMENT = 1 << 0;
        const STOP_ENABLE = 1 << 1;
        const MEASUREMENT_MODE_1 = 0 << 2; // Режим 1
        const MEASUREMENT_MODE_2 = 1 << 2; // Режим 2
        const MEASUREMENT_MODE_3 = 2 << 2; // Режим 3
        const FORCE_TRIGGER = 1 << 4;
        const EXT_START_ENABLE = 1 << 5;
        const EXT_STOP_ENABLE = 1 << 6;
        const SCLK_DIVIDER_1 = 0 << 7; // Делитель SCLK: /1
        const SCLK_DIVIDER_2 = 1 << 7; // Делитель SCLK: /2
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct Config2: u8 {
        const CLOCK_IN_EN = 1 << 0;
        const TEMP_COMPENSATION_EN = 1 << 1;
        const REPEAT_MEASUREMENT = 1 << 2;
        const CALIBRATION_MODE_SINGLE = 0 << 3; // Single-shot calibration
        const CALIBRATION_MODE_CONT = 1 << 3;   // Continuous calibration
        const CALIBRATION_FREQ_4MHZ = 0 << 4; // 4 MHz calibration frequency
        const CALIBRATION_FREQ_1MHZ = 1 << 4; // 1 MHz calibration frequency
        const NUM_STOP_3 = 0 << 6; // Number of STOPs: 3
        const NUM_STOP_2 = 1 << 6; // Number of STOPs: 2
        const NUM_STOP_1 = 2 << 6; // Number of STOPs: 1
        const NUM_STOP_4 = 3 << 6; // Number of STOPs: 4
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct MainControl: u8{
        const SW_RESET = 1 << 0;
        const CAL_START = 1 << 1;
        const MEASURE_START = 1 << 2;
        const ABORT = 1 << 3;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct InterruptStatus: u8{
        const NEW_MEASUREMENT = 1 << 0;
        const MEASUREMENT_COMPLETE = 1 << 1;
        const CALIBRATION_COMPLETE = 1 << 2;
        const COARSE_COUNTER_OVERFLOW = 1 << 3;
        const TIMEOUT_ERROR = 1 << 4;
    }
}

// Определяем структуру драйвера
pub struct Tdc7200<SPI, CS> {
    spi: SPI,
    chip_select: CS,
    registers: *mut Tdc7200Registers, // Указатель на регистры
}

// Простой конструктор без trait bounds
impl<SPI, CS> Tdc7200<SPI, CS> {
    /// Создает новый экземпляр драйвера TDC7200.
    pub fn new(spi: SPI, chip_select: CS) -> Self {
        Tdc7200 {
            spi,
            chip_select,
            registers: core::ptr::null_mut::<Tdc7200Registers>(),
        }
    }
}

// Реализуем методы для драйвера
impl<SPI, CS, SpiError, PinError> Tdc7200<SPI, CS>
where
    SPI: Transfer<u8, Error = SpiError> + Write<u8, Error = SpiError>,
    CS: OutputPin<Error = PinError>,
    SpiError: From<PinError>,
{
    /// Инициализирует TDC7200 с заданными настройками.
    pub fn init(
        &mut self,
        config1: Config1,
        config2: Config2,
        main_control: MainControl,
    ) -> Result<(), SpiError> {
        self.write_register(0x00, config1.bits())?;
        self.write_register(0x01, config2.bits())?;
        self.write_register(0x02, main_control.bits())?;
        Ok(())
    }

    /// Выполняет программный сброс устройства.
    pub fn reset(&mut self) -> Result<(), SpiError> {
        self.write_register(0x02, MainControl::SW_RESET.bits())?;
        self.write_register(0x02, 0x00)?;
        Ok(())
    }

    /// Запускает процесс калибровки.
    pub fn start_calibration(&mut self) -> Result<(), SpiError> {
        self.write_register(0x02, MainControl::CAL_START.bits())?;
        Ok(())
    }

    /// Запускает измерение.
    pub fn start_measurement(&mut self) -> Result<(), SpiError> {
        self.write_register(0x02, MainControl::MEASURE_START.bits())?;
        Ok(())
    }

    /// Прерывает текущую операцию.
    pub fn abort_operation(&mut self) -> Result<(), SpiError> {
        self.write_register(0x02, MainControl::ABORT.bits())?;
        Ok(())
    }

    /// Читает регистр TDC7200.
    fn read_register(&mut self, address: u8) -> Result<u8, SpiError> {
        let mut buffer = [address | 0x40, 0x00];
        self.chip_select.set_low().map_err(SpiError::from)?;
        self.spi.transfer(&mut buffer)?;
        self.chip_select.set_high().map_err(SpiError::from)?;
        Ok(buffer[1])
    }

    /// Записывает значение в регистр TDC7200.
    fn write_register(&mut self, address: u8, value: u8) -> Result<(), SpiError> {
        self.chip_select.set_low().map_err(SpiError::from)?; // Преобразование ошибки
        self.spi.write(&[address & 0x3F, value])?;
        self.chip_select.set_high().map_err(SpiError::from)?; // Преобразование ошибки
        Ok(())
    }

    /// Читает 16-битное значение из регистра.
    fn read_u16(&mut self, address: u8) -> Result<u16, SpiError> {
        let mut buffer = [address | 0x40, 0x00, 0x00];
        self.chip_select.set_low().map_err(SpiError::from)?;
        self.spi.transfer(&mut buffer)?;
        self.chip_select.set_high().map_err(SpiError::from)?;
        Ok(u16::from_be_bytes([buffer[1], buffer[2]]))
    }

    /// Читает 32-битное значение из регистра.
    fn read_u32(&mut self, address: u8) -> Result<u32, SpiError> {
        let mut buffer = [address | 0x40, 0x00, 0x00, 0x00, 0x00];
        self.chip_select.set_low().map_err(SpiError::from)?;
        self.spi.transfer(&mut buffer)?;
        self.chip_select.set_high().map_err(SpiError::from)?;
        Ok(u32::from_be_bytes([
            buffer[1], buffer[2], buffer[3], buffer[4],
        ]))
    }

    /// Получает значение Measurement1.
    pub fn get_measurement1(&mut self) -> Result<u32, SpiError> {
        self.read_u32(0x0A)
    }

    /// Получает значение Measurement2.
    pub fn get_measurement2(&mut self) -> Result<u32, SpiError> {
        self.read_u32(0x0C)
    }

    /// Получает значение регистра Interrupt Status.
    pub fn get_interrupt_status(&mut self) -> Result<InterruptStatus, SpiError> {
        let status = self.read_register(0x06)?;
        Ok(InterruptStatus::from_bits_truncate(status))
    }

    /// Устанавливает маску прерываний.
    pub fn set_interrupt_mask(&mut self, mask: u8) -> Result<(), SpiError> {
        self.write_register(0x05, mask)?;
        Ok(())
    }

    /// Очищает бит статуса прерывания.
    pub fn clear_interrupt_status(
        &mut self,
        interrupt_status: InterruptStatus,
    ) -> Result<(), SpiError> {
        let current_status = self.get_interrupt_status()?;
        self.write_register(0x06, current_status.bits() & !interrupt_status.bits())?;
        Ok(())
    }

    /// Устанавливает значение таймаута для первого измерения.
    pub fn set_timeout1(&mut self, timeout: u16) -> Result<(), SpiError> {
        let bytes = timeout.to_be_bytes();
        self.write_register(0x07, bytes[0])?;
        self.write_register(0x08, bytes[1])?;
        Ok(())
    }

    /// Устанавливает значение таймаута для второго измерения.
    pub fn set_timeout2(&mut self, timeout: u16) -> Result<(), SpiError> {
        let bytes = timeout.to_be_bytes();
        self.write_register(0x09, bytes[0])?;
        self.write_register(0x0A, bytes[1])?;
        Ok(())
    }

    /// Читает регистр Clock Count Overflow/Error Status.
    pub fn get_clock_count_overflow_error_status(&mut self) -> Result<u8, SpiError> {
        self.read_register(0x0B)
    }

    /// Читает регистр Number of Coarse Conversions.
    pub fn get_number_of_coarse_conversions(&mut self) -> Result<u8, SpiError> {
        self.read_register(0x0C)
    }

    /// Читает регистр Coarse Counter Overflow Count 1.
    pub fn get_coarse_counter_overflow_count1(&mut self) -> Result<u8, SpiError> {
        self.read_register(0x0E)
    }

    /// Читает регистр Coarse Counter Overflow Count 2.
    pub fn get_coarse_counter_overflow_count2(&mut self) -> Result<u8, SpiError> {
        self.read_register(0x0F)
    }

    /// Читает регистр Reference Clock Counter.
    pub fn get_reference_clock_counter(&mut self) -> Result<u32, SpiError> {
        self.read_u32(0x10)
    }
}
