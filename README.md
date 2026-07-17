# statecraft

[![CI](https://github.com/Sebkd/statecraft/actions/workflows/ci.yml/badge.svg)](https://github.com/Sebkd/statecraft/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.96.1-blue.svg)](https://github.com/rust-lang/rust/blob/master/RELEASES.md)

Ergonomic, compile-time validated async finite state machines for Rust.

`statecraft` turns an ordinary Rust `impl` block into a state machine with
generated state/event types, direct async event application, self-driving
handlers, and compile-time-checked branching — while keeping handler logic as
plain, explicit Rust.

> **Status:** early development. The core model below works today; the Tokio
> adapter and richer validation are still being built out. Design discussion
> welcome.

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
use statecraft::fsm;

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

A self-emitted event with no handler in the current state is skipped and logged
at `WARN` (via `tracing`), so it stays observable in production. A runaway
cascade is capped (default 10 000 events per `apply`, → `ApplyError::CascadeOverflow`).

## Features & configuration

- `public-emit` (default off): makes the generated `emit` method `pub`. By
  default it is module-private, callable only from handlers.
- `STATECRAFT_CASCADE_LIMIT` (compile-time env): overrides the self-emit cascade
  limit. `0` disables the limit (unbounded cascades).

## License

Licensed under the [MIT License](./LICENSE).
