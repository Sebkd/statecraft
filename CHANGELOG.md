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

[Unreleased]: http://sebkd.fvds.ru/sebkd/statecraft/compare/main...HEAD
