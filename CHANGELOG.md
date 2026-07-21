# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

> A Russian mirror is kept in [CHANGELOG_RU.md](CHANGELOG_RU.md).

## [Unreleased]

### Changed
- Clearer compile errors for common handler mistakes, pointing at the handler
  rather than at generated code: a fallible handler with no `type Error`; a
  payload event whose handler omits the payload argument; and a handler whose
  `Result` error type differs from `type Error` (now a single `mismatched types`
  error, expected/found).

## [0.1.2]

### Changed
- Internal: the `statecraft-macros` crate is split into `attrs` / `model` /
  `codegen` modules. No public API or behavior change.

## [0.1.1]

### Added
- Initial workspace scaffolding: the `statecraft` and `statecraft-macros` crates.
- D2: `#[fsm]`/`#[on]` generate the state and event enums and an async `apply`.
  For `#[on(next = [..])]` a per-transition target enum is generated, so
  returning an undeclared state is a compile error.
- `ApplyError::NoTransition` for a `(state, event)` pair with no handler.
- Fallible handlers: `Result<_, E>` returns (single and branching). `E` comes
  from `type Error`; handler errors surface through `apply` as
  `ApplyError::Handler`. `ApplyError` is now generic — `ApplyError<E = Infallible>`.
- D1 self-emit: handlers call `self.emit(event)` to queue a follow-up event for
  their own FSM. Emitted events are deferred and processed FIFO after the current
  handler's transition, within one `apply`. A self-emitted event with no handler
  is logged at `WARN` and skipped; a runaway cascade is capped
  (`ApplyError::CascadeOverflow`, default 10 000).
- `public-emit` feature (default off) — makes `emit` public.
- `STATECRAFT_CASCADE_LIMIT` (compile-time env) tunes the cascade limit; `0`
  disables it (unbounded cascades).
- Event payloads: `#[on(event = Foo(Type), ...)]`. The payload is passed to the
  handler by value; works with branching and `self.emit`. The same event name
  with different payload types is a compile error. Payload types must be `Debug`
  and at least as visible as the FSM.
- `self.emit_replace(event)` — priority self-emit: clears the pending self-emit
  queue and enqueues a single new event (when the queue is no longer relevant).
- D3 — optional Tokio adapter behind the `tokio` feature (default off):
  `Machine::spawn(ctx) -> ({Fsm}Handle, JoinHandle)`. `Handle` (Clone): `send`
  (fire-and-forget), `watch` (current state), `shutdown` (graceful),
  `shutdown_now` (hard). An `apply` error in the task is logged at `error!` and
  the task keeps running. Channel capacity via `#[fsm(channel_size = N)]`,
  default 64.

### Changed
- `tracing` is now a required dependency of `statecraft` (was an optional
  feature), so warnings about unhandled self-emits are always emitted.
- The generated event enum now derives only `Debug` (was
  `Debug, Clone, Copy, PartialEq, Eq`) — payloads may not support those traits.
- `tokio` is now an **optional** dependency (behind the `tokio` feature); the
  core is runtime-agnostic and builds without tokio. Removed the unused
  `tokio-util`.

[Unreleased]: https://github.com/Sebkd/statecraft/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.2
[0.1.1]: https://github.com/Sebkd/statecraft/releases/tag/v0.1.1
