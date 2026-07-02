//! Procedural macros for [`statecraft`](https://crates.io/crates/statecraft).
//!
//! The public surface is the [`macro@fsm`] attribute macro, which turns an
//! annotated `impl` block into a generated finite state machine.
//!
//! This is currently a skeleton: the macro validates that it is applied to an
//! `impl` block and re-emits it unchanged. Codegen (state/event enums,
//! `apply`, the Tokio adapter, and compile-time graph validation) is built out
//! from here.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemImpl, parse_macro_input};

/// Attribute macro that generates a finite state machine from an `impl` block.
///
/// ```ignore
/// #[fsm(initial = Idle)]
/// impl MyFsm {
///     type Context = MyContext;
///     type Error = std::convert::Infallible;
///
///     #[on(state = Idle, event = Start, next = Running)]
///     async fn handle_start(&mut self) {}
/// }
/// ```
#[proc_macro_attribute]
pub fn fsm(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    // Skeleton: re-emit the impl untouched. Real codegen lands here.
    let expanded = quote! {
        #input
    };

    expanded.into()
}
