# uFlowmeter

Встраиваемая система измерения расхода жидкости с использованием ультразвуковых датчиков на базе микроконтроллера STM32L151.

## Описание

uFlowmeter — это система измерения расхода жидкости методом времени прохождения (transit-time-of-flight), реализованная на Rust для платформы STM32. Система использует чипсеты TDC1000/TDC7200 для точного измерения времени прохождения ультразвукового сигнала и вычисления скорости потока.

### Ключевые возможности

- **Точное измерение**: Использование TDC7200 для высокоточного измерения времени
- **Двунаправленное измерение**: Поддержка измерения в обоих направлениях потока
- **Хранение данных**: История измерений (почасовая, ежедневная, ежемесячная) в энергонезависимой памяти
- **Интерфейс**: LCD дисплей 1602 с клавиатурой для управления
- **Modbus RTU**: Протокол для удаленного мониторинга и конфигурации
- **Управление питанием**: Низкое энергопотребление с поддержкой режима сна
- **Тестируемость**: Модульная архитектура с unit-тестами для основной логики

## Аппаратная платформа

- **MCU**: STM32L151C6 (Cortex-M3, 256KB Flash, 32KB RAM)
- **AFE**: TDC1000 — аналоговый фронтенд для управления ультразвуковыми датчиками
- **TDC**: TDC7200 — прецизионный преобразователь время-цифра
- **Дисплей**: LCD 1602 символьный дисплей
- **Память**: Microchip 25LC1024 (128KB EEPROM) для хранения истории и конфигурации
- **Интерфейсы**: 
  - SPI для связи с TDC чипсетами и EEPROM
  - UART для Modbus RTU
  - GPIO для клавиатуры и управления питанием

## Структура проекта

```
uflowmeter/
├── src/
│   ├── main.rs              # Основное приложение (RTIC)
│   ├── lib.rs               # Библиотечный интерфейс для тестирования
│   ├── apps.rs              # Логика приложений (измерение, настройки)
│   ├── ui.rs                # UI фреймворк
│   ├── gui/                 # Виджеты GUI (Label, Edit, и т.д.)
│   ├── hardware/            # Драйверы оборудования
│   │   ├── tdc1000.rs       # Драйвер TDC1000
│   │   ├── tdc7200.rs       # Драйвер TDC7200
│   │   ├── hd44780.rs       # Драйвер LCD
│   │   └── pins.rs          # Конфигурация пинов
│   ├── history.rs           # Система истории (embedded)
│   ├── history_lib.rs       # Система истории (testable)
│   ├── modbus.rs            # Реализация Modbus RTU
│   ├── modbus_handler.rs    # Обработчик Modbus запросов
│   └── measurement/         # Алгоритмы измерения расхода
├── examples/                # Примеры использования
├── docs/                    # Документация
│   ├── ARCHITECTURE.md      # Архитектура системы
│   ├── MODBUS_MAP.md        # Карта регистров Modbus
│   ├── TESTING.md           # Руководство по тестированию
│   └── ...
├── tests/                   # Интеграционные тесты
├── Cargo.toml               # Конфигурация зависимостей
├── memory.x                 # Карта памяти для линкера
├── .embed.toml              # Конфигурация для cargo-embed
└── Makefile                 # Команды сборки и тестирования
```

## Сборка и прошивка

### Требования

- Rust toolchain (рекомендуется rustup)
- `thumbv7m-none-eabi` target
- cargo-embed или probe-rs для прошивки

```bash
# Установка target
rustup target add thumbv7m-none-eabi

# Установка cargo-embed (опционально)
cargo install cargo-embed
```

### Сборка

```bash
# Release сборка
make build
# или
cargo build --release

# Debug сборка
cargo build
```

### Прошивка

```bash
# Используя cargo-embed
cargo embed --release

# Или используя probe-rs напрямую
probe-rs run --chip STM32L151C6 target/thumbv7m-none-eabi/release/uflowmeter
```

## Тестирование

Проект поддерживает тестирование на хост-платформе благодаря модульной архитектуре.

```bash
# Запуск всех тестов
make test

# Запуск тестов Modbus
make test-modbus

# Запуск тестов с подробным выводом
make test-modbus-verbose

# Запуск clippy
make clippy
```

### UI примеры

```bash
# Запуск UI примеров на хост-платформе
make ui-examples
```

Подробнее о тестировании см. в [docs/TESTING.md](docs/TESTING.md).

## Архитектура

Проект использует RTIC (Real-Time Interrupt-driven Concurrency) для управления задачами в реальном времени:

- **Измерительная задача**: Периодическое измерение потока
- **UI задача**: Обработка клавиатуры и обновление дисплея
- **Modbus задача**: Обработка запросов по UART
- **История**: Автоматическое сохранение данных в EEPROM

Подробную документацию по архитектуре см. в:
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/UI_ARCHITECTURE.md](docs/UI_ARCHITECTURE.md)
- [docs/HISTORY_SYSTEM.md](docs/HISTORY_SYSTEM.md)

## Modbus интерфейс

Устройство поддерживает Modbus RTU для удаленного мониторинга:

- **Скорость**: 9600 бод, 8N1
- **Адрес устройства**: Настраивается (по умолчанию 1)
- **Функции**: 0x03 (Read Holding Registers), 0x06 (Write Single Register), 0x10 (Write Multiple Registers)

Карту регистров см. в [docs/MODBUS_MAP.md](docs/MODBUS_MAP.md).

## Примеры

Доступные примеры в директории `examples/`:

- `display_example.rs` — Пример работы с LCD дисплеем
- `ui_examples.rs` — Демонстрация UI виджетов (для хост-платформы)
- `ui_examples_embedded.rs` — UI виджеты для встраиваемой системы
- `options_example.rs` — Работа с настройками системы
- `power_management_example.rs` — Управление питанием
- `ultrasonic_flow_example.rs` — Пример измерения расхода

См. также [examples/README.md](examples/README.md).

## Отладка

Проект использует defmt для логирования через RTT (Real-Time Transfer):

```bash
# Запуск с RTT логированием
cargo embed --release
```

Логи будут отображаться в терминале с временными метками.

## Зависимости

Основные зависимости:

- `stm32l1xx-hal` — HAL для STM32L1xx
- `cortex-m-rtic` — RTIC фреймворк
- `microchip-eeprom-25lcxx` — Драйвер EEPROM
- `embedded-hal` — Абстракции встраиваемого HAL
- `time` — Работа с датой/временем
- `defmt` — Эффективное логирование для встраиваемых систем

Полный список см. в [Cargo.toml](Cargo.toml).

## Документация

Полная документация доступна в директории `docs/`:

- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — Архитектура системы
- [HARDWARE_INTEGRATION.md](docs/HARDWARE_INTEGRATION.md) — Интеграция с аппаратурой
- [TESTING.md](docs/TESTING.md) — Руководство по тестированию
- [MODBUS_MAP.md](docs/MODBUS_MAP.md) — Карта Modbus регистров
- [TDC1000_REGISTER_MAP.md](docs/TDC1000_REGISTER_MAP.md) — Регистры TDC1000
- [TDC7200_REGISTER_MAP.md](docs/TDC7200_REGISTER_MAP.md) — Регистры TDC7200

## Лицензия

MIT OR Apache-2.0

## Автор

Sergej Lepin <sergej.lepin@gmail.com>
