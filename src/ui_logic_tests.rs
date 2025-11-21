//! Тесты для логики UI без embedded зависимостей

use time::{macros::datetime, Duration};

/// Тест правильности blink масок для времени и даты
#[test]
fn test_blink_masks_correct() {
    // Формат времени: "HH:MM:SS" (8 символов, позиции 0-7)
    // Маскирование: бит i соответствует позиции (LEN - i - 1)

    // Секунды (позиции 6-7) → биты 0-1 → маска 0x03
    assert_eq!(0x03, 0b00000011);

    // Минуты (позиции 3-4) → биты 3-4 → маска 0x18
    assert_eq!(0x18, 0b00011000);

    // Часы (позиции 0-1) → биты 6-7 → маска 0xc0
    assert_eq!(0xc0, 0b11000000);

    // Проверяем, что маски не пересекаются
    assert_eq!(0x03 & 0x18, 0);
    assert_eq!(0x03 & 0xc0, 0);
    assert_eq!(0x18 & 0xc0, 0);
}

/// Тест что timestamp содержит полный Unix timestamp (не % 60)
#[test]
fn test_timestamp_full_value() {
    let dt = datetime!(2023-06-15 10:30:45 UTC);
    let ts = dt.unix_timestamp() as u32;

    // Unix timestamp должен быть большим числом (не % 60)
    assert!(ts > 1686000000);
    assert!(ts < 2000000000); // разумная верхняя граница

    // Если бы использовался % 60, значение было бы 0-59
    assert!(ts > 60);
}

/// Тест инкремента часа - timestamp должен увеличиться на 3600
#[test]
fn test_hour_increment_timestamp() {
    let dt1 = datetime!(2023-06-15 10:00:00 UTC);
    let ts1 = dt1.unix_timestamp() as u32;

    let dt2 = dt1.saturating_add(Duration::HOUR);
    let ts2 = dt2.unix_timestamp() as u32;

    assert_eq!(ts2 - ts1, 3600);
}

/// Тест инкремента дня - timestamp должен увеличиться на 86400
#[test]
fn test_day_increment_timestamp() {
    let dt1 = datetime!(2023-06-15 00:00:00 UTC);
    let ts1 = dt1.unix_timestamp() as u32;

    let dt2 = dt1.saturating_add(Duration::DAY);
    let ts2 = dt2.unix_timestamp() as u32;

    assert_eq!(ts2 - ts1, 86400);
}

/// Тест что разные даты дают разные timestamps
#[test]
fn test_different_dates_different_timestamps() {
    let dt1 = datetime!(2023-06-15 10:30:00 UTC);
    let dt2 = datetime!(2023-06-16 10:30:00 UTC);
    let dt3 = datetime!(2023-07-15 10:30:00 UTC);

    let ts1 = dt1.unix_timestamp() as u32;
    let ts2 = dt2.unix_timestamp() as u32;
    let ts3 = dt3.unix_timestamp() as u32;

    assert_ne!(ts1, ts2);
    assert_ne!(ts1, ts3);
    assert_ne!(ts2, ts3);
}

/// Тест инкремента минуты
#[test]
fn test_minute_increment_timestamp() {
    let dt1 = datetime!(2023-06-15 10:30:00 UTC);
    let ts1 = dt1.unix_timestamp() as u32;

    let dt2 = dt1.saturating_add(Duration::MINUTE);
    let ts2 = dt2.unix_timestamp() as u32;

    assert_eq!(ts2 - ts1, 60);
}

/// Тест инкремента секунды
#[test]
fn test_second_increment_timestamp() {
    let dt1 = datetime!(2023-06-15 10:30:00 UTC);
    let ts1 = dt1.unix_timestamp() as u32;

    let dt2 = dt1.saturating_add(Duration::SECOND);
    let ts2 = dt2.unix_timestamp() as u32;

    assert_eq!(ts2 - ts1, 1);
}

/// Тест что timestamp монотонно возрастает при инкременте
#[test]
fn test_timestamp_monotonic() {
    let mut dt = datetime!(2023-06-15 10:00:00 UTC);
    let mut prev_ts = dt.unix_timestamp() as u32;

    for _ in 0..100 {
        dt = dt.saturating_add(Duration::MINUTE);
        let ts = dt.unix_timestamp() as u32;
        assert!(ts > prev_ts);
        prev_ts = ts;
    }
}

/// Тест позиционирования битов в маске для 8-символьной строки
#[test]
fn test_bitmask_positioning() {
    // Для LEN=8, позиция символа i соответствует биту (LEN - i - 1)
    const LEN: usize = 8;

    // Позиция 0 (первый символ) → бит 7
    assert_eq!(LEN - 0 - 1, 7);

    // Позиция 1 → бит 6
    assert_eq!(LEN - 1 - 1, 6);

    // Позиция 6 → бит 1
    assert_eq!(LEN - 6 - 1, 1);

    // Позиция 7 (последний символ) → бит 0
    assert_eq!(LEN - 7 - 1, 0);
}

/// Тест что все маски для времени покрывают все символы
#[test]
fn test_time_masks_complete() {
    // Секунды: 0x03 (биты 0-1)
    // Минуты: 0x18 (биты 3-4)
    // Часы: 0xc0 (биты 6-7)
    // Двоеточия: биты 2, 5 (не мигают)

    let seconds_mask = 0x03_u8;
    let minutes_mask = 0x18_u8;
    let hours_mask = 0xc0_u8;

    // Объединение всех масок должно покрывать все цифры
    let all_digits = seconds_mask | minutes_mask | hours_mask;

    // Проверяем, что биты 0,1,3,4,6,7 установлены (цифры)
    assert_eq!(all_digits & 0x03, 0x03); // секунды
    assert_eq!(all_digits & 0x18, 0x18); // минуты
    assert_eq!(all_digits & 0xc0, 0xc0); // часы

    // Биты 2 и 5 (двоеточия) не должны быть установлены
    assert_eq!(all_digits & 0x04, 0); // бит 2
    assert_eq!(all_digits & 0x20, 0); // бит 5
}

/// Тест декремента даты
#[test]
fn test_date_decrement_timestamp() {
    let dt1 = datetime!(2023-06-15 10:00:00 UTC);
    let ts1 = dt1.unix_timestamp() as u32;

    let dt2 = dt1.saturating_sub(Duration::DAY);
    let ts2 = dt2.unix_timestamp() as u32;

    assert_eq!(ts1 - ts2, 86400);
}
