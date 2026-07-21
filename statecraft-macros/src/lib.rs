//! Procedural macros for [`statecraft`](https://crates.io/crates/statecraft).
//!
//! The public surface is the [`macro@fsm`] attribute macro, which turns an
//! annotated `impl` block into a generated finite state machine.
//!
//! # Scope
//!
//! The macro generates the state/event enums, an owned (runtime-agnostic)
//! `apply`, and a per-transition target enum for every `#[on(next = [..])]`
//! with more than one target (returning an undeclared target is a compile
//! error). It also supports fallible handlers (`Result<_, Error>`), event
//! payloads (`event = Foo(Type)`), self-emit (`self.emit`) for FIFO follow-up
//! events, and an optional Tokio adapter behind the `tokio` feature.
//!
//! Modules: [`attrs`] parses `#[on]`; [`model`] holds the discovered handlers
//! and events plus validation; [`codegen`] emits the generated code.

mod attrs;
mod codegen;
mod model;

use proc_macro::TokenStream;
use syn::{Ident, ItemImpl, parse_macro_input};

/// Attribute macro that generates a finite state machine from an `impl` block.
///
/// ```ignore
/// #[fsm(initial = Idle)]
/// impl MyFsm {
///     type Context = MyContext;
///
///     #[on(state = Idle, event = Start, next = Running)]
///     async fn handle_start(&mut self) {}
///
///     #[on(state = Running, event = Check, next = [Running, Done])]
///     async fn handle_check(&mut self) -> RunningCheckNext {
///         RunningCheckNext::Done
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn fsm(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut initial: Option<Ident> = None;
    let mut channel_size: usize = 64;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("initial") {
            initial = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("channel_size") {
            channel_size = meta.value()?.parse::<syn::LitInt>()?.base10_parse()?;
            Ok(())
        } else {
            Err(meta.error("unsupported #[fsm] key (expected `initial`, `channel_size`)"))
        }
    });
    parse_macro_input!(attr with parser);
    let input = parse_macro_input!(item as ItemImpl);

    let Some(initial) = initial else {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[fsm] requires `initial = <State>`",
        )
        .to_compile_error()
        .into();
    };

    match codegen::expand(&initial, channel_size, &input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
