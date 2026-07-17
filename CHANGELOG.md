# Changelog

Все заметные изменения проекта фиксируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект придерживается [семантического версионирования](https://semver.org/lang/ru/).

## [Unreleased]

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

### Changed
- `tracing` теперь обязательная зависимость `statecraft` (была опциональной
  фичей), чтобы предупреждения о необработанных self-emit были видны всегда.
- Сгенерированный Event-enum теперь `#[derive(Debug)]` только (раньше
  `Debug, Clone, Copy, PartialEq, Eq`) — payload может не поддерживать эти
  трейты.

[Unreleased]: http://sebkd.fvds.ru/sebkd/statecraft/compare/main...HEAD
