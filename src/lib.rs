//! # statecraft
//!
//! Ergonomic, compile-time validated async finite state machines for Rust.
//!
//! `statecraft` turns an ordinary Rust `impl` block into a state machine with
//! generated state and event types, direct async event application, self-driving
//! handlers, and compile-time-checked branching — while keeping handler logic as
//! plain, explicit Rust.
//!
//! ## Example
//!
//! ```ignore
//! use statecraft_fsm::fsm;
//!
//! #[derive(Debug, Default)]
//! pub struct MyContext {
//!     count: usize,
//! }
//!
//! #[fsm(initial = Idle)]
//! impl MyFsm {
//!     type Context = MyContext;
//!
//!     #[on(state = Idle, event = Start, next = Running)]
//!     async fn on_start(&mut self) {
//!         self.context.count += 1;
//!     }
//!
//!     #[on(state = Running, event = Stop, next = Idle)]
//!     async fn on_stop(&mut self) {}
//! }
//!
//! // The macro generates `MyFsmState`, `MyFsmEvent`, and the `MyFsm` struct.
//! // Drive it yourself: `fsm.apply(MyFsmEvent::Start).await?;`
//! ```
//!
//! ## Features & configuration
//!
//! - **`tokio`** (default off): the opt-in Tokio adapter — `Machine::spawn`
//!   returns a cloneable `Handle` (`send`/`watch`/`shutdown`) plus a
//!   `JoinHandle`. Without it, only the runtime-agnostic owned core is compiled
//!   and `tokio` is not a dependency.
//! - **`public-emit`** (default off): makes the generated `emit` /
//!   `emit_replace` methods `pub`. By default they are module-private, callable
//!   only from handlers.
//! - **`#[fsm(channel_size = N)]`**: capacity of the spawned event channel
//!   (default 64; Tokio adapter only).
//! - **`STATECRAFT_CASCADE_LIMIT`** (compile-time env): caps self-emit cascades
//!   per `apply` (default 10_000); `0` disables the limit.
//!
//! ## Fallible handlers
//!
//! A handler may return `Result<_, E>`, where `E` is the FSM's `type Error`.
//! The return type must be spelled with a `Result` path segment (`Result<_, E>`
//! or `core::result::Result<_, E>`): the macro does not resolve type aliases, so
//! a handler returning e.g. `MyResult<()>` is **not** recognized as fallible.

pub use statecraft_fsm_macros::fsm;

/// Error returned by a generated `apply`.
///
/// The type parameter `E` is the FSM's declared `type Error` (defaulting to
/// [`Infallible`](core::convert::Infallible) when no handler is fallible).
///
/// Note that an invalid *target* is not representable here: because branch
/// targets are encoded in a generated per-transition enum (see the
/// `#[on(next = [..])]` form), returning an undeclared target is a compile
/// error, not a runtime `ApplyError`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError<E = core::convert::Infallible> {
    /// No handler is declared for this event in the current state.
    #[error("no transition declared for this event in the current state")]
    NoTransition,
    /// A handler returned an error.
    #[error(transparent)]
    Handler(E),
    /// A single `apply` processed more self-emitted events than the configured
    /// cascade limit allows (see [`cascade_limit`]). Guards against a handler
    /// that emits an event which re-triggers itself indefinitely.
    #[error("self-emit cascade exceeded the configured limit")]
    CascadeOverflow,
}

/// Default upper bound on the number of self-emitted events one `apply` call
/// will process before returning [`ApplyError::CascadeOverflow`].
pub const DEFAULT_CASCADE_LIMIT: usize = 10_000;

/// Resolve the cascade limit from an optional environment value, baked in at
/// compile time via `option_env!("STATECRAFT_CASCADE_LIMIT")`.
///
/// - absent or non-numeric → [`DEFAULT_CASCADE_LIMIT`]
/// - `0` → no limit (unbounded cascades)
///
/// Not intended to be called directly; the `#[fsm]` macro emits the call.
#[doc(hidden)]
pub const fn cascade_limit(env: Option<&str>) -> usize {
    match env {
        Some(s) => parse_usize_or(s, DEFAULT_CASCADE_LIMIT),
        None => DEFAULT_CASCADE_LIMIT,
    }
}

const fn parse_usize_or(s: &str, default: usize) -> usize {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return default;
    }
    let mut acc: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            return default;
        }
        acc = acc * 10 + (b - b'0') as usize;
        i += 1;
    }
    acc
}

/// Emit the structured warning for a self-emitted event that has no handler in
/// the current state. Called by generated code; logs unconditionally at `WARN`
/// so it is observable in production without enabling verbose tracing.
#[doc(hidden)]
pub fn __unhandled_emit(
    fsm: &'static str,
    state: &dyn core::fmt::Debug,
    event: &dyn core::fmt::Debug,
) {
    tracing::warn!(
        fsm = fsm,
        state = ?state,
        event = ?event,
        "statecraft: self-emitted event has no handler in the current state; skipped",
    );
}

/// Log an `apply` failure inside a spawned FSM task. Spawned FSMs log the error
/// and keep running (they do not stop on a failed transition).
#[doc(hidden)]
pub fn __log_spawn_error<E: core::fmt::Debug>(fsm: &'static str, err: &ApplyError<E>) {
    tracing::error!(
        fsm = fsm,
        error = ?err,
        "statecraft: apply failed in spawned FSM; continuing",
    );
}

/// Runtime primitives re-exported for generated Tokio-adapter code. Only present
/// with the `tokio` feature. Not a stable public API.
#[cfg(feature = "tokio")]
#[doc(hidden)]
pub mod __rt {
    pub use tokio::sync::{Notify, mpsc, watch};
    pub use tokio::task::{AbortHandle, JoinHandle};
    pub use tokio::{select, spawn};

    /// Error returned by `Handle::send` when the FSM task has stopped.
    pub type SendError<T> = tokio::sync::mpsc::error::SendError<T>;
}
