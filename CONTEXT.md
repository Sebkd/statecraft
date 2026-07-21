# CONTEXT — project map for statecraft

Orientation for contributors and agents: what lives where and how the pieces fit.
For build/test/style conventions see [AGENTS.md](AGENTS.md); for user-facing docs
see [README.md](README.md).

## What this is

`statecraft` turns an `impl` block into an async finite state machine. You write
handlers as ordinary `async fn` methods; the `#[fsm]` attribute macro generates
the state/event enums, the FSM struct, `apply`, self-emit, compile-time-checked
branching, and an optional Tokio adapter.

## Workspace layout

Two crates (`Cargo.toml` workspace):

- **`statecraft`** (`src/lib.rs`) — the facade + runtime core. Re-exports the
  `fsm` macro and holds the small runtime support the generated code calls.
- **`statecraft-macros`** (`statecraft-macros/src/`) — the proc-macro crate.

### `src/lib.rs` (core)

- `pub use statecraft_macros::fsm;` — the public macro.
- `ApplyError<E = Infallible>` — `NoTransition` / `Handler(E)` / `CascadeOverflow`.
- `DEFAULT_CASCADE_LIMIT`, `cascade_limit(Option<&str>)` — compile-time self-emit
  cascade cap (via `option_env!("STATECRAFT_CASCADE_LIMIT")`).
- `__unhandled_emit(...)`, `__log_spawn_error(...)` — `#[doc(hidden)]` helpers the
  generated code calls for tracing (WARN / ERROR).
- `#[cfg(feature = "tokio")] mod __rt` — re-exported Tokio primitives
  (`mpsc`/`watch`/`Notify`/`spawn`/`JoinHandle`/`AbortHandle`/`select!`) so the
  generated adapter needs no direct tokio dependency in the user crate.

### `statecraft-macros/src/` (proc-macro)

- **`lib.rs`** — the `#[proc_macro_attribute] fsm` entry point; parses the
  `#[fsm(initial = .., channel_size = ..)]` args and calls `codegen::expand`.
- **`attrs.rs`** — parses `#[on(state = .., event = .., next = ..)]` (`OnAttr`,
  `parse_on`) and the fallibility heuristic (`returns_result`).
- **`model.rs`** — the discovered model: `Handler`, `EventDef`, and helpers
  (`add_unique`, `add_event` payload-consistency check, `next_enum_ident`).
- **`validation.rs`** — parse-time checks with clear, handler-spanned errors:
  payload arity, fallible-without-`type Error`, Next-enum name collisions.
- **`codegen.rs`** — `expand`: generates the enums, the FSM struct,
  `new`/`state`/`emit`/`emit_replace`/`apply`, and (behind `cfg!(feature =
  "tokio")`) the `spawn` + `Handle` adapter.

## The generated FSM

From `impl MyFsm { ... }` the macro generates:

- `enum MyFsmState { .. }` — `Debug, Clone, Copy, PartialEq, Eq`.
- `enum MyFsmEvent { .. }` — `Debug` only (payloads may not be Copy/Eq/Clone).
- `enum {State}{Event}Next { .. }` — per branching `#[on(next = [..])]`; the
  handler returns it, so an undeclared target is a compile error.
- `struct MyFsm { pub state, pub context, __queue }` with `new`, `state`,
  `emit`/`emit_replace` (private unless `public-emit`), `apply`.
- with the `tokio` feature: `struct MyFsmHandle` + `MyFsm::spawn`.

### Handler return shapes

- single `next = X`: `()` or `Result<(), Error>`.
- branching `next = [..]`: `{State}{Event}Next` or `Result<{State}{Event}Next, Error>`.
- payload event `event = Foo(T)`: handler takes `T` by value.

## Owned vs spawned

- **Owned** (default): you hold the value and call `fsm.apply(ev).await`. No
  runtime dependency; drive it with any executor. One owner (`&mut`).
- **Spawned** (`tokio` feature): `MyFsm::spawn(ctx)` runs the FSM in a background
  task; talk to it via a cloneable `Handle` (`send` fire-and-forget, `watch`
  state, `shutdown`/`shutdown_now`). See memory: fire-and-forget and
  log+continue are fixed architectural decisions.

Self-emit (`emit`) uses one internal FIFO `VecDeque` drained inside each `apply`
in both modes (a hardcoded FIFO/breadth-first invariant).

## Features & env

- `tokio` — the adapter (default off; core is runtime-agnostic).
- `public-emit` — make `emit`/`emit_replace` public (default: module-private).
- `serde` — (declared; reserved).
- `STATECRAFT_CASCADE_LIMIT` — compile-time env, self-emit cascade cap; `0` = off.

## Tests

- `tests/` — integration tests (`d1_*`, `d2_*`, `payload`, `tokio_adapter`).
  The adapter tests are `#![cfg(feature = "tokio")]`; run with
  `cargo test --features tokio --test tokio_adapter`.
- `statecraft-macros/tests/` — `trybuild` harness: `pass/` (must compile) and
  `ui/` (must fail, with `.stderr` snapshots for each diagnostic). Run trybuild
  under default features only (the `tokio`/`public-emit` features alter
  generated code and would break the snapshots).

## Examples

- `examples/` — runnable examples (`cargo run --example <name>`), plus the
  `examples/axum_fsm/` crate for the web-server example.

## Where to look

- change parsing of `#[on]` → `attrs.rs`; add a validation/diagnostic →
  `validation.rs`; change generated code → `codegen.rs`; runtime types/helpers →
  `src/lib.rs`.
- task list / decisions → `docs/backlog.md` (internal); design → `DESIGN.md`
  (internal).
