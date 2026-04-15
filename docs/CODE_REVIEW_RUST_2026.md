# UFlowMeter Rust — Code Review 2026

**Дата:** 2026-04-15  
**Рецензент:** Макс (AI, 20+ лет embedded стаж)  
**Версия:** main branch, commit cc6f183  
**Размер:** ~9К строк Rust, ~2600 строк тестов  

---

## Общая оценка

Проект — Rust-порт ультразвукового расходомера на STM32L151 с RTIC framework. Архитектура в целом разумная, модульность хорошая, тесты есть. Но несколько критических gaps: реальные измерения не подключены, Modbus не интегрирован, unsafe-коды без safety comments. До продакшна ещё существенная работа.

---

## 🔴 CRITICAL

### 1. Нет реальных измерений — flow генерируется RNG

**Файл:** `src/main.rs:448`  
**Компонент:** Измерение

```rust
app.flow = rng.next_u32() as f32 / 1_000_000.0;
```

TDC1000/TDC7200 драйверы написаны (~750 строк), класс `UltrasonicFlowMeter` существует (~736 строк), но **ни один не подключён к RTIC-задачам**. Расходомер не измеряет — это заглушка.

**Рекомендация:** Создать RTIC task для измерений (SPI + TDC ISR), подключить `UltrasonicFlowMeter` к `app_request::Process`.

---

### 2. 14 `unwrap()` в `init()` — любой паникует

**Файл:** `src/main.rs` (строки 222, 226, 254, 262, 271, 273, 282, 322-324, 422)  
**Компонент:** Инициализация

EEPROM, serial, storage, ADC — всё `.unwrap()`. На проде это = мёртвый прибор без диагностики. Если EEPROM битый или SPI не отвечает — panic без лога.

**Рекомендация:** Заменить на `match` / `if let` с `defmt::error!` и fallback-значениями. Для критических ошибок — войти в safe mode с минимальным UI.

---

### 3. Modbus НЕ интегрирован в RTIC

**Файл:** `src/modbus.rs` + `src/modbus_handler.rs`  
**Компонент:** Коммуникация

Полноценная реализация Modbus RTU (~1230 строк), юнит-тесты есть. Но:
- Нет UART ISR в RTIC (`binds = USART1` отсутствует)
- Нет serial receive task
- Нет вызова `ModbusHandler::handle_request()` из main loop
- **Modbus — мёртвый код**

**Рекомендация:** Добавить `#[task(binds = USART1)]` для приёма UART, буферизацию фреймов по таймауту 3.5 символа, и вызов `handle_request()` с ответом через UART TX.

---

## 🟠 HIGH

### 4. `unsafe` в `edit.rs` — `as_bytes_mut()` на String

**Файл:** `src/gui/edit.rs:118`  
**Компонент:** UI

```rust
unsafe {
    let bytes = state.as_bytes_mut();
    for (i, item) in bytes.iter_mut().enumerate().take(LEN) {
        if self.blink_mask.get_bit(LEN - i - 1) {
            *item = b' ';
        }
    }
}
```

Это **Undefined Behaviour** если строка содержит multi-byte UTF-8 (кириллица!). Русский шрифт уже есть в проекте, Edit может получить кириллицу → замена байтов сломает UTF-8 кодировку → panic при следующем обращении к строке.

**Рекомендация:** Работать с `char`-ами, а не байтами. Или хранить blink-маску как `Vec<char>` / массив `char`.

---

### 5. `unsafe` в `gpio_power.rs` — 11 unsafe блоков без Safety Comments

**Файл:** `src/hardware/gpio_power.rs`  
**Компонент:** Power management

Чтение/запись регистров GPIO напрямую через `$GPIOX::ptr()`. Для stop-mode это обосновано (нужно сохранить/восстановить полное состояние GPIO), но:
- Нет `// SAFETY:` комментариев
- Commented-out код с `RCC::ptr()` — значит был unsafe доступ к RCC

**Рекомендация:** Добавить Safety Comments к каждому unsafe блоку. Рассмотреть `cortex_m::peripheral::MODIFY` через svd2rust API.

---

### 6. Power management — `wfi()` через внутренний API RTIC

**Файл:** `src/hardware/power.rs`  
**Компонент:** Power management

```rust
rtic::export::wfi();
```

`rtic::export` — внутренний API, не входит в public API RTIC. При обновлении `cortex-m-rtic` это может сломаться.

**Рекомендация:** Использовать `cortex_m::asm::wfi()` напрямую, или реализовать через `#[idle]` handler с explicit sleep.

---

### 7. History `add()` — нет ограничений на EEPROM writes

**Файл:** `src/history.rs:161-202`  
**Компонент:** History storage

При gap-fill (пропущенные интервалы) пишутся нули на каждый пропущенный период. Если прибор был выключен месяц — сотни zero-записей в EEPROM. Нет:
- Throttle (максимум N writes за вызов)
- Wear-leveling защиты
- Проверки на переполнение flash

**Рекомендация:** Добавить лимит gap-fill (например, максимум 24 записи за вызов). Если gap больше — сбросить буфер и начать заново (как уже делается при `delta / ELEMENT_SIZE >= SIZE`).

---

## 🟡 MEDIUM

### 8. `emballoc` — 4KB heap, нет OOM-обработки

**Файл:** `src/main.rs:54`  
**Компонент:** Memory

```rust
static ALLOCATOR: emballoc::Allocator<4096> = emballoc::Allocator::new();
```

`App` использует `alloc::string::String`. На Cortex-M3 с 32KB RAM это ок, но:
- Нет проверки allocation failure
- `String::push_str` может panic при OOM
- Нет мониторинга использования heap

**Рекомендация:** Перейти на `heapless::String` где возможно. Для App — использовать фиксированные буферы.

---

### 9. Baud rate 112500 — нестандартный

**Файл:** `src/main.rs:269`  
**Компонент:** Serial / Modbus

```rust
serial::Config::default().baudrate(112500)
```

Стандартные Modbus baud rates: 9600, 19200, 38400, 115200. 112500 — нестандартный. Похоже на опечатку.

**Рекомендация:** Проверить — должно быть 115200?

---

### 10. Нет watchdog (IWDG)

**Компонент:** Безопасность

C++ версия тоже без WD, но для расходомера это риск — зависший прибор молчит и не измеряет. STM32L151 имеет IWDG.

**Рекомендация:** Добавить IWDG с таймаутом ~5 сек, кикать в main loop / RTIC idle.

---

### 11. Нет `#[idle]` handler в RTIC

**Компонент:** Power management

Нет явного idle loop. После завершения задач MCU крутится в дефолтном idle. При включенной `low_power` фиче — может не входить в stop mode корректно.

**Рекомендация:** Добавить `#[idle]` с `cortex_m::asm::wfi()` для explicit sleep.

---

### 12. Тесты требуют sed-хака `.cargo/config.toml`

**Файл:** `run_tests.sh`, `Makefile`  
**Компонент:** Build / CI

Скрипты правят `.cargo/config.toml` на лету (удаляют `target = "thumbv7m-none-eabi"`) чтобы запустить тесты на host. Хрупко — при падении конфиг не восстанавливается (хотя есть `|| ` fallback).

**Рекомендация:** Использовать `cargo test --target x86_64-apple-darwin` (или `x86_64-unknown-linux-gnu` в CI) с отдельным `.cargo/config-test.toml`. Или настроить `cfg(test)` правильно.

---

## 🟢 LOW / INFO

### 13. Глобальные `#![allow(dead_code)]` + `#![allow(unused_imports)]`

Прототип — ок. Перед продом убрать и почистить.

---

### 14. Архитектура в README говорит STM32F103

**Файл:** `docs/ARCHITECTURE.md`  
Реально STM32L151. Нужно поправить.

---

### 15. Закомментированный код

CRC-функция, EEPROM-тесты, BTreeMap-эксперимент, RCC-доступ — всё закомментировано. Почистить перед ревью.

---

## 📋 Что не завершено vs C++ версия

| Функция | C++ | Rust |
|---------|-----|------|
| Реальные измерения TDC | ✅ | ⚠️ TDC1000/TDC7200 ISR подключены, flow=0.0 (нужна формула) |
| Modbus RTU интеграция | ✅ | ✅ USART1 ISR + modbus_poll task |
| M-Bus | ✅ | ✅ mbus.rs + CommType switching |
| Shell/CLI | ✅ | ✅ shell.rs + ASCII/Modbus mux |
| Калибровка | ✅ | ❌ нет |
| Калькулятор потока | ✅ | ⚠️ код есть (ultrasonic_flow.rs), не подключён |
| Low power | ✅ | ✅ #[idle] + WFI |
| Watchdog | ❌ | ✅ IWDG 5с |

---

## 🎯 Приоритет доработки

1. **Подключить TDC1000/TDC7200 к RTIC** — реальные измерения вместо RNG
2. **Интегрировать Modbus** — serial task + UART ISR + handle_request
3. **Починить unsafe в Edit** — UTF-8 safety
4. **Убрать unwrap() в init()** — graceful degradation
5. **Добавить IWDG** — watchdog для надёжности
6. **Поправить baud rate** — 115200 вместо 112500
7. **Добавить #[idle] handler** — корректный low power
8. **Safety Comments** — ко всем unsafe блокам

---

_Ревью подготовлено Максом ⚡ для Серджа_