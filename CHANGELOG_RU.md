# Changelog (RU)

Русское зеркало [CHANGELOG.md](CHANGELOG.md) (основной ведётся на английском).

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект придерживается [семантического версионирования](https://semver.org/lang/ru/).

## [0.1.6]

### Removed
- Фича `serde`. Была объявлена, но ничего не делала (никаких `serde`-derive не
  генерировалось); убрана, чтобы не тащить пустую фичу.

### Changed
- Доки: поправлена заметка о статусе в README (Tokio-адаптер готов, а не «ещё в
  разработке») и добавлено упоминание `tokio-fsm` как источника идеи.

## [0.1.5]

### Added
- Примеры: `showcase` (self-emit, ветвление, `emit_replace`),
  `driven_by_channel`, `stream_file` и HTTP-интеграция с axum
  (`examples/axum_fsm/`).
- `CONTEXT.md` — карта кодовой базы для контрибьюторов и агентов.

## [0.1.4]

### Added
- Ошибка компиляции, когда две ветвящиеся транзиции порождают одинаковое имя
  target-enum (напр. `(AB, C)` и `(A, BC)` дают `ABCNext`) — вместо
  непонятной ошибки о повторном определении.

## [0.1.3]

### Changed
- Внятные ошибки компиляции для типичных ошибок в хендлерах — с указанием на сам
  хендлер, а не на сгенерированный код: fallible-хендлер без `type Error`;
  payload-событие, хендлер которого не принимает аргумент payload; и хендлер,
  тип ошибки `Result` которого не совпадает с `type Error` (теперь один
  `mismatched types`, expected/found).

## [0.1.2]

### Changed
- Внутреннее: крейт `statecraft-macros` разбит на модули `attrs` / `model` /
  `codegen`. Публичный API и поведение не изменились.

## [0.1.1]

### Added
- Начальный каркас workspace: крейты `statecraft` и `statecraft-macros`.
- Прототип D2: `#[fsm]`/`#[on]` генерируют enum'ы состояний и событий и
  асинхронный `apply`. Для `#[on(next = [..])]` создаётся per-transition
  enum-цель, поэтому возврат необъявленного состояния — ошибка компиляции.
- `ApplyError::NoTransition` для пары `(state, event)` без хендлера.
- Fallible-хендлеры: возврат `Result<_, E>` (одиночный и ветвящийся). Тип `E`
  задаётся `type Error`, ошибка прорастает через `apply` как
  `ApplyError::Handler`. `ApplyError` стал generic — `ApplyError<E = Infallible>`.
- D1 self-emit: хендлеры вызывают `self.emit(event)`, чтобы поставить своей же
  FSM последующее событие. События отложенные и обрабатываются FIFO после
  перехода текущего хендлера, в рамках одного `apply`. Наэмиченное событие без
  хендлера логируется на `WARN` и пропускается; runaway-каскад ограничен
  (`ApplyError::CascadeOverflow`, дефолт 10 000).
- Фича `public-emit` (default off) — делает `emit` публичным.
- Env `STATECRAFT_CASCADE_LIMIT` (compile-time) настраивает лимит каскада;
  `0` — без лимита.
- Payload у событий: `#[on(event = Foo(Type), ...)]`. Payload передаётся в
  хендлер аргументом по значению; работает с ветвлением и `self.emit`. Одно имя
  события с разными payload-типами — ошибка компиляции. Payload-типы должны быть
  `Debug` и не менее видимы, чем FSM.
- `self.emit_replace(event)` — приоритетный self-emit: сбрасывает очередь
  наэмиченных событий и ставит одно новое (когда очередь стала неактуальной).
- D3 — опциональный Tokio-адаптер за фичей `tokio` (default off):
  `Machine::spawn(ctx) -> ({Fsm}Handle, JoinHandle)`. `Handle` (Clone): `send`
  (fire-and-forget), `watch` (текущее состояние), `shutdown` (graceful),
  `shutdown_now` (hard). Ошибка `apply` в фоне логируется на `error!` и задача
  продолжает работу. Ёмкость канала — `#[fsm(channel_size = N)]`, default 64.

### Changed
- `tracing` теперь обязательная зависимость `statecraft` (была опциональной
  фичей), чтобы предупреждения о необработанных self-emit были видны всегда.
- Сгенерированный Event-enum теперь `#[derive(Debug)]` только (раньше
  `Debug, Clone, Copy, PartialEq, Eq`) — payload может не поддерживать эти
  трейты.
- `tokio` теперь **опциональная** зависимость (за фичей `tokio`); ядро
  runtime-agnostic и собирается без tokio. Убрана неиспользуемая `tokio-util`.

[0.1.6]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.6
[0.1.5]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.5
[0.1.4]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.4
[0.1.3]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.3
[0.1.2]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.2
[0.1.1]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.1
