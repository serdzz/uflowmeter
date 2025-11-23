# Исправления критических ошибок бизнес-логики

## Дата: 2025-11-23

### 1. ❗ КРИТИЧЕСКАЯ: Сброс аккумуляторов потока после сохранения

**Файл:** `src/main.rs:454-503`

**Проблема:**
После сохранения часовых/дневных/месячных данных счётчики `hour_flow`, `day_flow`, `month_flow` не сбрасывались. Это приводило к накоплению значений и дублированию данных в истории.

**Пример:**
- Час 1: flow = 10 L → hour_flow = 10, сохранено 10
- Час 2: flow = 15 L → hour_flow = 25 (10+15), сохранено 25 ❌
- Час 3: flow = 20 L → hour_flow = 45 (10+15+20), сохранено 45 ❌

**Исправление:**
Добавлен сброс аккумуляторов после успешного сохранения:
```rust
if let Err(_e) = (hour_history, &mut storage).lock(|hour_history, storage| {
    hour_history.add(storage, hour_flow as i32, timestamp as u32)
}) {
    defmt::error!("Failed to log hour flow:");
} else {
    defmt::info!("Hour flow logged: {} at {}", hour_flow, timestamp);
    // ✅ Сброс часового аккумулятора
    app.lock(|app| app.hour_flow = 0.0);
}
```

Аналогично для `day_flow` и `month_flow`.

---

### 2. ❗ Неправильная обработка отрицательной дельты времени

**Файл:** `src/history.rs:181-204`

**Проблема:**
При записи данных с timestamp меньше последнего (откат времени) логика добавляла `ELEMENT_SIZE` к абсолютному значению дельты:
```rust
delta = delta.abs() + ELEMENT_SIZE;  // ❌ Ошибка
while delta != 0 {
    // ...
    delta -= ELEMENT_SIZE;
}
```

Это могло привести к:
- Записи лишних нулевых значений
- Некорректному уменьшению размера буфера
- Потере данных

**Исправление:**
```rust
delta = delta.abs();  // ✅ Убрали + ELEMENT_SIZE
while delta >= ELEMENT_SIZE {  // ✅ Изменили условие
    // ...
}
```

---

### 3. ❗ Переполнение при уменьшении offset

**Файл:** `src/history.rs:194-198`

**Проблема:**
При offset = 0 операция `offset - 1` вызывала underflow u32:
```rust
let tmp = self.data.offset_of_last() - 1;  // ❌ Underflow при offset=0
self.data.set_offset_of_last(tmp);
if self.data.offset_of_last() > SIZE as u32 {
    self.data.set_offset_of_last(SIZE as u32);
}
```

Проверка `> SIZE` не помогала, т.к. u32::MAX > SIZE.

**Исправление:**
```rust
// ✅ Явная проверка на ноль
if self.data.offset_of_last() == 0 {
    self.data.set_offset_of_last(SIZE as u32 - 1);
} else {
    let tmp = self.data.offset_of_last() - 1;
    self.data.set_offset_of_last(tmp);
}
```

---

### 4. ❗ Некорректное заполнение пропусков в истории

**Файл:** `src/history.rs:164-175`

**Проблема:**
При пропуске временных интервалов пропущенные слоты заполнялись нулями с timestamp = 0:
```rust
while delta > ELEMENT_SIZE {
    self.write_value(storage, 0, 0)?;  // ❌ timestamp = 0!
    delta -= ELEMENT_SIZE;
    self.advance_offset_by_one();
}
```

Это приводило к:
- Некорректным timestamp в базе
- Невозможности точно определить время пропуска
- Ошибкам при поиске по времени

**Исправление:**
```rust
while delta > ELEMENT_SIZE {
    // ✅ Вычисляем корректный timestamp для пропуска
    let gap_time = self.data.time_of_last() + ELEMENT_SIZE as u32;
    self.write_value(storage, 0, gap_time)?;
    self.write_service_data(storage)?;
    delta -= ELEMENT_SIZE;
}
self.write_value(storage, val, time)?;
self.write_service_data(storage)?;
return Ok(());
```

---

### 5. ❗ Неправильное условие проверки offset

**Файл:** `src/history.rs:189`

**Проблема:**
Сравнение offset с size логически неверно:
```rust
if self.data.offset_of_last() == self.data.size() {  // ❌
```

- `offset_of_last()` - позиция в кольцевом буфере (0..SIZE-1)
- `size()` - количество элементов (0..SIZE)

Они имеют разный диапазон значений.

**Исправление:**
```rust
if self.data.offset_of_last() == self.data.size() - 1 {  // ✅
```

---

## Тестирование

После внесения исправлений:
- ✅ Проект компилируется без ошибок
- ✅ Clippy не выдаёт предупреждений
- ✅ Размер бинарника: 60612 bytes (Flash), 6852 bytes (RAM)

## Рекомендации для дальнейшей разработки

1. **Добавить unit-тесты** для модуля `history.rs`:
   - Тест заполнения пропусков
   - Тест отрицательной дельты времени
   - Тест переполнения кольцевого буфера

2. **Добавить флаги "уже записано"** в основном цикле:
   ```rust
   let mut last_hour_logged = 0;
   if datetime.time().minute() == 0 && last_hour_logged != datetime.time().hour() {
       // Запись в историю
       last_hour_logged = datetime.time().hour();
   }
   ```

3. **Добавить валидацию timestamp** перед записью:
   - Проверка на разумный диапазон
   - Защита от слишком больших скачков времени

4. **Логирование для отладки:**
   - Добавить debug-логи при сбросе аккумуляторов
   - Логировать пропущенные интервалы времени
