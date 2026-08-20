# Tasks: fix-fsm-stack-overflow

> **Статус**: T1–T6 и T8 выполнены. Открыты T7 (сторона потребителя) и
> архивация change'а.

---

## T1: Разбор флага `boxed` в `#[on]`

**Файл**: `statecraft-fsm-macros/src/attrs.rs`

- [x] Добавить поле `boxed: bool` в `OnAttr`
- [x] В `parse_on` — ветка на path-only ключ `boxed` (без `= value`)
- [x] Обновить текст ошибки неизвестного ключа: `expected state, event, next, boxed`

**Acceptance**: `#[on(state = A, event = E, next = B, boxed)]` парсится; `boxxed`
даёт ошибку с перечислением допустимых ключей.

---

## T2: Возможности сборки `boxed-all` и `diagnostics`

**Файлы**: `Cargo.toml`, `statecraft-fsm-macros/Cargo.toml`

- [x] Объявить `boxed-all` и `diagnostics` в макро-крейте (пустые списки)
- [x] Пробросить обе из фасада (`boxed-all = ["statecraft-fsm-macros/boxed-all"]`),
      по образцу `public-emit`
- [x] Обе выключены по умолчанию

**Acceptance**: `cargo test --features boxed-all` и `--features diagnostics`
собираются; `cargo tree -f "{p} {f}"` подтверждает проброс в макро-крейт.

---

## T3: Обёртка вызова в плече `match`

**Файл**: `statecraft-fsm-macros/src/codegen.rs`

- [x] Вынести формирование вызова обработчика в общий узел `__call`
- [x] Оборачивать в `::std::boxed::Box::pin(...)` при
      `h.on.boxed || cfg!(feature = "boxed-all")`
- [x] Убедиться, что fallible-ветка осталась внутри `quote_spanned!{h.span=> …}`

**Acceptance**: `cargo expand` показывает `Box::pin(self.<handler>(…)).await`
на помеченном переходе и на любом переходе при `--features boxed-all`; на
непомеченном переходе в сборке по умолчанию — прежний код без изменений.

---

## T4: Диагностика — `apply_future_size`

**Файлы**: `statecraft-fsm-macros/src/codegen.rs`

- [x] Генерировать `pub fn apply_future_size(&mut self, event) -> usize` под
      возможностью `diagnostics` (объявлена в T2)
- [x] Документировать в `src/lib.rs`, раздел «Features & configuration»

**Acceptance**: с `--features diagnostics` функция доступна и возвращает то же
значение, что `size_of_val` вручную; без возможности — не генерируется.

---

## T5: Тесты

**Файлы**: `tests/boxed_dispatch.rs`, `statecraft-fsm-macros/tests/ui/*.rs`,
`examples/stack_frame.rs`, `.github/workflows/ci.yml`

- [x] D5.1, политика по умолчанию — три проверки размера с абсолютными порогами
- [x] D5.1, оптовая политика — две проверки под `#[cfg(feature = "boxed-all")]`
- [x] D5.1 — тест семантической нейтральности переключения (критерий приёмки №6)
- [x] D5.2 — subprocess-тест на реальный `stack_size = 64 КБ`
- [x] D5.3 — trybuild-тест на опечатку в ключе `#[on]`
- [x] D5.4 — вся матрица сборок из D3 зелёная **без правок существующих тестов**
- [x] D5.5 — `examples/stack_frame.rs`, воспроизводящий таблицы замеров change.md
- [x] CI: добавить джобы на `--features boxed-all,diagnostics` и `--all-features`
- [x] `cargo clippy --all-targets --all-features` без новых предупреждений

**Acceptance**: критерии приёмки 1–11 из `change.md` покрыты тестами.

**Отклонения от плана, зафиксированные при выполнении:**

- Subprocess-тесты стека помечены `#[ignore]` и гоняются отдельной CI-джобой в
  debug-сборке. Причина не только в шуме от намеренного падения дочернего
  процесса: в release-сборке два случая не разделяются вовсе — оптимизатор
  строит по месту и небоксированную future. Подробности и таблица — `design.md`,
  D5.2.
- Бюджет стека в тесте — 4 МБ, а не 64 КБ: 64 КБ не проходит даже с
  боксированием, потому что построение future обработчика всё равно задевает
  стек (см. границу требования в §1 `change.md`).
- Замеры обязаны использовать `std::hint::black_box` на крупных локалах, иначе
  release-сборка вычёркивает их и стенд показывает несуществующий выигрыш.
  Первая редакция change'а содержала такие цифры; они исправлены.

---

## T6: Документация и релиз

- [x] `README.md` — раздел про стоимость стека, когда ставить `boxed` и когда
      включать `boxed-all`; явно про остаточный риск унификации фичи (§2.3 change.md)
- [x] `src/lib.rs` (rustdoc) — `boxed`, `boxed-all`, `diagnostics`
- [x] Bump версии до 0.2.0 в `Cargo.toml` (workspace.package)

**Acceptance**: обе политики документированы в rustdoc и README, выбор между
ними описан с цифрами.

---

## T7: Довести до потребителя

- [ ] Опубликовать 0.2.0, обновить `statecraft-fsm` в `sepia`
- [ ] Пройти процедуру D4.2 в `sepia/process-driver`: замерить `apply_future_size`
      по каждой FSM **до** любых правок
- [ ] Если < 4 КБ — снять ручные `Box::pin` из 26 обработчиков и завести отдельный
      change на поиск настоящего источника переполнения
- [ ] Если велик — заменить ручные обёртки на `#[on(..., boxed)]`, повторить замер
- [ ] Закрепить порог тестом в CI потребителя

**Acceptance**: причина падения `process-driver` установлена **замером**, а не
перебором обходных путей.

---

## T8: Влить изменение в спецификацию

- [x] `openspec/specs/statecraft/statecraft.md` — переписать 4.3.4 с
      осведомлённости на требование (§6 change.md)
- [x] Там же — 2.5 (параметр `boxed`), 2.8 (две возможности сборки),
      4.2 (стоимость боксируемого перехода), 4.4 (снять оговорку про диагностику)
- [x] Там же — критерии приёмки 1, 2, 4, 6, 9 из §4 change.md
- [x] Версия спеки поднята 1.0 → 1.1
- [ ] Перенести каталог change'а в
      `openspec/changes/archive/{YYYY-MM-DD}-fix-fsm-stack-overflow/`

**Acceptance**: поведение зафиксировано в спеке, change заархивирован.
