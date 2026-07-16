//! # statecraft
//!
//! Ergonomic, compile-time validated async finite state machines for Rust.
//!
//! `statecraft` turns an ordinary Rust `impl` block into a state machine with
//! generated state and event types, direct async event application, an optional
//! runtime adapter, and compile-time graph validation — while keeping handler
//! logic as plain, explicit Rust.
//!
//! This crate is in early development. The API below is the intended surface;
//! codegen is being built out behind the [`macro@fsm`] attribute.
//!
//! ## Example (target API)
//!
//! ```ignore
//! use statecraft::fsm;
//!
//! #[derive(Debug, Default)]
//! pub struct MyContext {
//!     count: usize,
//! }
//!
//! #[fsm(initial = Idle)]
//! impl MyFsm {
//!     type Context = MyContext;
//!     type Error = std::convert::Infallible;
//!
//!     #[on(state = Idle, event = Start, next = Running)]
//!     async fn handle_start(&mut self) {
//!         self.context.count += 1;
//!     }
//!
//!     #[on(state = Running, event = Stop, next = Idle)]
//!     async fn handle_stop(&mut self) {}
//! }
//! ```

pub use statecraft_macros::fsm;

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
