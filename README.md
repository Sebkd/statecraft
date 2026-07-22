# statecraft-fsm

[![CI](https://github.com/Sebkd/statecraft/actions/workflows/ci.yml/badge.svg)](https://github.com/Sebkd/statecraft/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.96.1-blue.svg)](https://github.com/rust-lang/rust/blob/master/RELEASES.md)

Ergonomic, compile-time validated async finite state machines for Rust.

`statecraft-fsm` turns an ordinary Rust `impl` block into a state machine with
generated state/event types, direct async event application, self-driving
handlers, and compile-time-checked branching — while keeping handler logic as
plain, explicit Rust.

> **Status:** early development. The core model, the self-emit engine, and the
> optional Tokio adapter all work today; broader graph validation (reachability,
> exhaustiveness) is still to come. Design discussion welcome.

## Install

```toml
[dependencies]
statecraft-fsm = "0.1"
```

The crate is published as `statecraft-fsm`; in code it is imported as
`statecraft_fsm` (e.g. `use statecraft_fsm::fsm;`).

## Goals

- **Runtime-agnostic core** — the FSM itself has no mandatory async runtime; a
  Tokio adapter is opt-in.
- **Compile-time safety** — branch targets are checked by the compiler, not at
  runtime.
- **Self-driving** — handlers can drive the FSM forward by emitting their own
  follow-up events, without wiring a handle into the context.
- **Explicit handlers** — no hidden control flow; handlers are ordinary
  `async fn` methods.

## Quick start

```rust
use statecraft_fsm::fsm;

#[derive(Debug, Default)]
pub struct MyContext {
    count: usize,
}

#[fsm(initial = Idle)]
impl MyFsm {
    type Context = MyContext;

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) {
        self.context.count += 1;
    }

    #[on(state = Running, event = Stop, next = Idle)]
    async fn on_stop(&mut self) {}
}

// The macro generates `MyFsmState`, `MyFsmEvent`, and the `MyFsm` struct:
let mut fsm = MyFsm::new(MyContext::default());
fsm.apply(MyFsmEvent::Start).await?;
assert_eq!(fsm.state(), MyFsmState::Running);
```

## Compile-time-checked branching

One `(state, event)` may lead to several `next` states. The macro generates a
per-transition target enum (`{State}{Event}Next`); the handler returns it.
Returning an undeclared target is a **compile error**, not a runtime failure.

```rust
#[fsm(initial = Idle)]
impl Worker {
    type Context = Ctx;

    #[on(state = Idle, event = Check, next = [Idle, Done])]
    async fn on_check(&mut self) -> IdleCheckNext {
        self.context.tries += 1;
        if self.context.tries >= 3 { IdleCheckNext::Done } else { IdleCheckNext::Idle }
        // `IdleCheckNext::Whatever` would not compile.
    }
}
```

## Fallible handlers

Handlers may return `Result<_, E>`. The error type comes from `type Error`, and
handler errors surface through `apply` as `ApplyError::Handler`.

```rust
#[derive(Debug)]
struct MyError;

#[fsm(initial = Idle)]
impl Job {
    type Context = ();
    type Error = MyError;

    #[on(state = Idle, event = Run, next = Done)]
    async fn on_run(&mut self) -> Result<(), MyError> {
        Ok(())
    }
}
```

## Event payloads

An event may carry data: `#[on(event = Foo(Type), ...)]`. The payload is passed
to the handler by value and works with branching and `self.emit`.

```rust
#[derive(Debug)]
pub struct Order {
    qty: u32,
}

#[fsm(initial = Idle)]
impl Shop {
    type Context = Totals;

    #[on(state = Idle, event = Add(Order), next = Idle)]
    async fn on_add(&mut self, order: Order) {
        self.context.total += order.qty;
    }
}
// shop.apply(ShopEvent::Add(Order { qty: 3 })).await?;
```

The same event name must declare the same payload everywhere (a mismatch is a
compile error). Payload types must implement `Debug` and be at least as visible
as the FSM (typically `pub`), since the generated event enum exposes them.

## Self-driving handlers (`self.emit`)

A handler can enqueue a follow-up event for its own FSM with `self.emit(...)`.
Emitted events are deferred: they run **after** the current handler returns and
its transition is applied, FIFO, all within the same `apply` call.

```rust
#[fsm(initial = Idle)]
impl Pipeline {
    type Context = ();

    #[on(state = Idle, event = Start, next = Working)]
    async fn on_start(&mut self) {
        self.emit(PipelineEvent::Work); // handled once we are in `Working`
    }

    #[on(state = Working, event = Work, next = Done)]
    async fn on_work(&mut self) {}
}
// A single `apply(Start)` walks Idle -> Working -> Done.
```

`self.emit_replace(event)` is a priority variant: it drops any pending
self-emitted events and keeps only this one — handy when a newly relevant event
makes the queued ones obsolete.

```rust
#[on(state = Running, event = Interrupt, next = Cancelling)]
async fn on_interrupt(&mut self) {
    // Whatever was queued no longer matters; go straight to cleanup.
    self.emit_replace(PipelineEvent::Cleanup);
}
```

A self-emitted event with no handler in the current state is skipped and logged
at `WARN` (via `tracing`), so it stays observable in production. A runaway
cascade is capped (default 10 000 events per `apply`, → `ApplyError::CascadeOverflow`).

## Owned mode (default, no Tokio)

By default there is no runtime and no background task: you own the FSM value (as
in [Quick start](#quick-start)) and drive it yourself with `apply`. This is the
runtime-agnostic core — `apply` is a plain `async fn`, so any executor works and
the crate does not depend on Tokio.

```rust
let mut fsm = MyFsm::new(MyContext::default());

fsm.apply(MyFsmEvent::Start).await?;   // one event + its self-emit cascade
let _ = fsm.state();                   // current state (Copy)
let _ = &fsm.context;                  // context is directly accessible

// drive it with whatever executor you like — Tokio, or none:
// futures::executor::block_on(fsm.apply(MyFsmEvent::Stop))?;
```

One owner drives it (`&mut fsm`); for concurrent sending from several tasks, use
spawned mode below.

## Spawned mode (optional Tokio adapter)

The core is runtime-agnostic — you drive it yourself with `apply`. With the
`tokio` feature, the FSM can instead run in a background Tokio task; you talk to
it through a cloneable `Handle` and observe state via `watch`.

```rust
// requires the `tokio` feature and a running Tokio runtime
let (handle, join) = Worker::spawn(());

handle.send(WorkerEvent::Start).await?;   // fire-and-forget

let mut states = handle.watch();          // watch::Receiver<WorkerState>
states.changed().await?;
assert_eq!(*states.borrow(), WorkerState::Running);

handle.shutdown();     // graceful: drain queued events, then stop
// handle.shutdown_now();  // hard: abort immediately
join.await?;
```

- `send` is fire-and-forget; observe the outcome via `watch`.
- A handler error in the background is logged at `error!` and the task keeps
  running.
- The event channel is bounded; set its capacity with `#[fsm(channel_size = N)]`
  (default 64).
- `Context` and the payload types must be `Send + 'static`.

## Features & configuration

- `tokio` (default off): the Tokio adapter (`spawn`/`Handle`/`watch`). Without
  it, only the owned core is compiled and `tokio` is not a dependency.
- `public-emit` (default off): makes the generated `emit` method `pub`. By
  default it is module-private, callable only from handlers.
- `#[fsm(channel_size = N)]`: capacity of the spawned event channel (default 64).
- `STATECRAFT_CASCADE_LIMIT` (compile-time env): overrides the self-emit cascade
  limit. `0` disables the limit (unbounded cascades).

## Examples

Runnable examples live in [`examples/`](./examples):

- **`showcase`** — owned mode: `self.emit`, multi-branch from one `(state,
  event)`, and `emit_replace`. `cargo run --example showcase`
- **`driven_by_channel`** — an owned FSM driven by an external event source (a
  timer feeding a channel). `cargo run --example driven_by_channel`
- **`stream_file`** — drive an FSM from a streamed file, with streaming I/O
  inside handlers. `cargo run --example stream_file`
- **`axum_fsm/`** — a registry (`HashMap`) of spawned FSMs, one per id, driven
  over an [axum](https://github.com/tokio-rs/axum) HTTP API. Keeps each
  `JoinHandle` for controlled shutdown — graceful `shutdown` + await with a
  hard `shutdown_now` timeout fallback (`DELETE` stops one, Ctrl-C drains all) —
  and shows multi-target branching. `cd examples/axum_fsm && cargo run`

See [CONTEXT.md](./CONTEXT.md) for a map of the codebase.

## Acknowledgements

`statecraft` was inspired by [tokio-fsm](https://github.com/abhishekshree/tokio-fsm)
(MIT-licensed), which explored the `#[fsm]` / `#[on]` attribute-macro approach to
async state machines in Rust. `statecraft` is an independent implementation with a
different design (runtime-agnostic core, compile-time-checked branching, self-emit
engine, opt-in Tokio adapter), but credit is due to that project for the idea.

## License

Licensed under the [MIT License](./LICENSE).
