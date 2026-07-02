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
