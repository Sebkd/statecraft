//! Procedural macros for [`statecraft`](https://crates.io/crates/statecraft).
//!
//! The public surface is the [`macro@fsm`] attribute macro, which turns an
//! annotated `impl` block into a generated finite state machine.
//!
//! # D2 prototype scope
//!
//! This is an early prototype focused on **D2 — compile-time-checked
//! branching**. It generates the state/event enums, an owned (runtime-agnostic)
//! `apply`, and a per-transition target enum for every `#[on(next = [..])]`
//! with more than one target. Returning an undeclared target is therefore a
//! compile error, not a runtime `InvalidTransition`.
//!
//! Deliberately out of scope for now: event payloads, `Result<_, Error>`
//! handler returns, the Tokio adapter (spawn/Handle), and `self.emit` (D1).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Ident, ImplItem, ItemImpl, Token, Type, bracketed, parse_macro_input,
    punctuated::Punctuated, spanned::Spanned, token::Bracket,
};

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
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("initial") {
            initial = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("unsupported #[fsm] key (expected `initial`)"))
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

    match expand(&initial, &input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parsed `#[on(state = .., event = .., next = ..)]` attribute.
struct OnAttr {
    state: Ident,
    event: Ident,
    next: Vec<Ident>,
}

fn parse_on(attr: &Attribute) -> syn::Result<OnAttr> {
    let mut state = None;
    let mut event = None;
    let mut next: Option<Vec<Ident>> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("state") {
            state = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("event") {
            event = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("next") {
            let input = meta.value()?;
            if input.peek(Bracket) {
                let content;
                bracketed!(content in input);
                let values = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                next = Some(values.into_iter().collect());
            } else {
                next = Some(vec![input.parse()?]);
            }
        } else {
            return Err(meta.error("unsupported #[on] key (expected state, event, next)"));
        }
        Ok(())
    })?;

    let span = attr.span();
    let next = next.ok_or_else(|| syn::Error::new(span, "missing #[on] key: next"))?;
    if next.is_empty() {
        return Err(syn::Error::new(
            span,
            "#[on] next must list at least one state",
        ));
    }
    Ok(OnAttr {
        state: state.ok_or_else(|| syn::Error::new(span, "missing #[on] key: state"))?,
        event: event.ok_or_else(|| syn::Error::new(span, "missing #[on] key: event"))?,
        next,
    })
}

/// One discovered handler: the method name, its `#[on]` metadata, and whether
/// it returns a `Result` (and thus may fail).
struct Handler {
    method: Ident,
    on: OnAttr,
    fallible: bool,
}

/// Heuristic: a handler is fallible when its return type is a path ending in
/// `Result` (e.g. `Result<T, E>` or `std::result::Result<T, E>`). Type aliases
/// are not resolved.
fn returns_result(sig: &syn::Signature) -> bool {
    match &sig.output {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => match &**ty {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "Result"),
            _ => false,
        },
    }
}

fn expand(initial: &Ident, input: &ItemImpl) -> syn::Result<TokenStream2> {
    let fsm_name = match &*input.self_ty {
        Type::Path(path) => path.path.segments.last().unwrap().ident.clone(),
        other => {
            return Err(syn::Error::new(
                other.span(),
                "#[fsm] must be applied to `impl <Name>`",
            ));
        }
    };

    let context_ty = input
        .items
        .iter()
        .find_map(|item| match item {
            ImplItem::Type(t) if t.ident == "Context" => Some(t.ty.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new(
                input.span(),
                "#[fsm] impl must declare `type Context = ...;`",
            )
        })?;

    // Optional `type Error`; defaults to `Infallible` when no handler fails.
    let error_ty: Type = input
        .items
        .iter()
        .find_map(|item| match item {
            ImplItem::Type(t) if t.ident == "Error" => Some(t.ty.clone()),
            _ => None,
        })
        .unwrap_or_else(|| syn::parse_quote!(::core::convert::Infallible));

    let mut handlers = Vec::new();
    for item in &input.items {
        if let ImplItem::Fn(f) = item {
            if let Some(attr) = f.attrs.iter().find(|a| a.path().is_ident("on")) {
                handlers.push(Handler {
                    method: f.sig.ident.clone(),
                    on: parse_on(attr)?,
                    fallible: returns_result(&f.sig),
                });
            }
        }
    }

    // States, in first-seen order: initial, then each handler's source and
    // targets. Events, likewise, from each handler's trigger.
    let mut states: Vec<Ident> = Vec::new();
    add_unique(&mut states, initial);
    let mut events: Vec<Ident> = Vec::new();
    for h in &handlers {
        add_unique(&mut states, &h.on.state);
        for target in &h.on.next {
            add_unique(&mut states, target);
        }
        add_unique(&mut events, &h.on.event);
    }

    let state_enum = format_ident!("{}State", fsm_name);
    let event_enum = format_ident!("{}Event", fsm_name);

    // Per-transition target enums: one for every branching handler.
    let next_enums = handlers.iter().filter(|h| h.on.next.len() > 1).map(|h| {
        let name = next_enum_ident(&h.on);
        let variants = &h.on.next;
        quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum #name {
                #(#variants,)*
            }
        }
    });

    // apply arms. Single-target handlers set the state directly; branching
    // handlers map the returned target enum onto a state. A fallible handler
    // has its error wrapped into `ApplyError::Handler` via `?`.
    let arms = handlers.iter().map(|h| {
        let method = &h.method;
        let s = &h.on.state;
        let ev = &h.on.event;
        // `self.#method().await`, plus `?`-unwrapping when the handler is fallible.
        let call = if h.fallible {
            quote! { self.#method().await.map_err(::statecraft::ApplyError::Handler)? }
        } else {
            quote! { self.#method().await }
        };
        let set_state = if h.on.next.len() == 1 {
            let target = &h.on.next[0];
            quote! {
                #call;
                self.state = #state_enum::#target;
            }
        } else {
            let next_name = next_enum_ident(&h.on);
            let map_arms =
                h.on.next
                    .iter()
                    .map(|t| quote! { #next_name::#t => #state_enum::#t, });
            quote! {
                let __next = #call;
                self.state = match __next { #(#map_arms)* };
            }
        };
        quote! {
            (#state_enum::#s, #event_enum::#ev) => {
                #set_state
                ::core::result::Result::Ok(())
            }
        }
    });

    // Re-emit handler methods without the `#[on]` attribute; drop associated
    // types (they only configure codegen).
    let cleaned: Vec<ImplItem> = input
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(f) => {
                let mut f = f.clone();
                f.attrs.retain(|a| !a.path().is_ident("on"));
                Some(ImplItem::Fn(f))
            }
            ImplItem::Type(_) => None,
            other => Some(other.clone()),
        })
        .collect();

    Ok(quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum #state_enum {
            #(#states,)*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum #event_enum {
            #(#events,)*
        }

        #(#next_enums)*

        pub struct #fsm_name {
            pub state: #state_enum,
            pub context: #context_ty,
        }

        impl #fsm_name {
            pub fn new(context: #context_ty) -> Self {
                Self { state: #state_enum::#initial, context }
            }

            pub fn state(&self) -> #state_enum {
                self.state
            }

            pub async fn apply(
                &mut self,
                event: #event_enum,
            ) -> ::core::result::Result<(), ::statecraft::ApplyError<#error_ty>> {
                match (self.state, event) {
                    #(#arms)*
                    _ => ::core::result::Result::Err(::statecraft::ApplyError::NoTransition),
                }
            }

            #(#cleaned)*
        }
    })
}

fn next_enum_ident(on: &OnAttr) -> Ident {
    format_ident!("{}{}Next", on.state, on.event)
}

fn add_unique(list: &mut Vec<Ident>, id: &Ident) {
    if !list.iter().any(|existing| existing == id) {
        list.push(id.clone());
    }
}
