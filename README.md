# statecraft

Ergonomic, compile-time validated async finite state machines for Rust.

`statecraft` turns an ordinary Rust `impl` block into a state machine with
generated state/event types, direct async event application, an optional
runtime adapter, and compile-time graph validation — while keeping handler
logic as plain, explicit Rust.

> **Status:** early development. The API below describes the intended surface;
> codegen is being built out. Contributions and design discussion welcome.

## Goals

- **Runtime-agnostic core** — the FSM itself has no mandatory async runtime; a
  Tokio adapter is opt-in.
- **Compile-time safety** — validate state reachability and transition
  contracts at compile time, with clear diagnostics.
- **Explicit handlers** — no hidden control flow; handlers are ordinary
  `async fn` methods.
- **Deterministic lifecycle** — clear ownership, predictable resource cleanup.

## Quick start (target API)

```rust
use statecraft::fsm;

#[derive(Debug, Default)]
pub struct MyContext {
    count: usize,
}

#[fsm(initial = Idle)]
impl MyFsm {
    type Context = MyContext;
    type Error = std::convert::Infallible;

    #[on(state = Idle, event = Start, next = Running)]
    async fn handle_start(&mut self) {
        self.context.count += 1;
    }

    #[on(state = Running, event = Stop, next = Idle)]
    async fn handle_stop(&mut self) {}
}
```

## License

Licensed under the [MIT License](./LICENSE).
