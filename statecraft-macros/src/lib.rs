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
//! payloads (`event = Foo(Type)`), and self-emit (`self.emit`) for FIFO
//! follow-up events.
//!
//! Not yet implemented: the Tokio adapter (spawn/Handle/watch).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Ident, ImplItem, ItemImpl, Token, Type, bracketed, parenthesized, parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::{Bracket, Paren},
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

    match expand(&initial, channel_size, &input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parsed `#[on(state = .., event = .., next = ..)]` attribute.
struct OnAttr {
    state: Ident,
    event: Ident,
    /// Payload type when the event is declared as `event = Foo(Type)`.
    payload: Option<Type>,
    next: Vec<Ident>,
}

fn parse_on(attr: &Attribute) -> syn::Result<OnAttr> {
    let mut state = None;
    let mut event = None;
    let mut payload: Option<Type> = None;
    let mut next: Option<Vec<Ident>> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("state") {
            state = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("event") {
            // `event = Foo` (unit) or `event = Foo(Type)` (payload).
            let input = meta.value()?;
            event = Some(input.parse()?);
            if input.peek(Paren) {
                let content;
                parenthesized!(content in input);
                payload = Some(content.parse::<Type>()?);
            }
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
        payload,
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

fn expand(initial: &Ident, channel_size: usize, input: &ItemImpl) -> syn::Result<TokenStream2> {
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
        if let ImplItem::Fn(f) = item
            && let Some(attr) = f.attrs.iter().find(|a| a.path().is_ident("on"))
        {
            handlers.push(Handler {
                method: f.sig.ident.clone(),
                on: parse_on(attr)?,
                fallible: returns_result(&f.sig),
            });
        }
    }

    // States, in first-seen order: initial, then each handler's source and
    // targets. Events, likewise, from each handler's trigger.
    let mut states: Vec<Ident> = Vec::new();
    add_unique(&mut states, initial);
    let mut events: Vec<EventDef> = Vec::new();
    for h in &handlers {
        add_unique(&mut states, &h.on.state);
        for target in &h.on.next {
            add_unique(&mut states, target);
        }
        add_event(&mut events, &h.on)?;
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
        // Payload events bind their value and pass it to the handler by value.
        let (ev_pat, call_arg) = if h.on.payload.is_some() {
            (quote! { #event_enum::#ev(__payload) }, quote! { __payload })
        } else {
            (quote! { #event_enum::#ev }, quote! {})
        };
        // `self.#method(<arg>).await`, plus `?`-unwrapping when fallible.
        let call = if h.fallible {
            quote! { self.#method(#call_arg).await.map_err(::statecraft::ApplyError::Handler)? }
        } else {
            quote! { self.#method(#call_arg).await }
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
            (#state_enum::#s, #ev_pat) => {
                #set_state
                ::core::result::Result::Ok(::core::option::Option::None)
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

    // `emit` visibility is opt-in via the `public-emit` feature (forwarded from
    // the `statecraft` facade). Default: module-private, so only handlers reach it.
    let emit_vis = if cfg!(feature = "public-emit") {
        quote! { pub }
    } else {
        quote! {}
    };
    let fsm_name_str = fsm_name.to_string();

    // The event enum carries only `Debug` (payloads may not be Copy/Eq/Clone).
    // Internally we never require more (see the copy-free dispatch below).
    let event_variants = events.iter().map(|e| {
        let name = &e.name;
        match &e.payload {
            Some(ty) => quote! { #name(#ty), },
            None => quote! { #name, },
        }
    });

    // Tokio adapter (feature `tokio`): `spawn` + a cloneable `Handle`. When the
    // feature is off, `cfg!` is false and empty token streams are emitted, so
    // the owned core stays runtime-agnostic.
    let handle_name = format_ident!("{}Handle", fsm_name);
    let (adapter_impl_items, adapter_types) = if cfg!(feature = "tokio") {
        let impl_items = quote! {
            /// Spawn this FSM onto a Tokio task. Returns a cloneable handle and
            /// the task's `JoinHandle`. Requires a running Tokio runtime.
            pub fn spawn(
                context: #context_ty,
            ) -> (#handle_name, ::statecraft::__rt::JoinHandle<()>)
            where
                #context_ty: ::core::marker::Send + 'static,
                #event_enum: ::core::marker::Send + 'static,
            {
                let (__tx, mut __rx) =
                    ::statecraft::__rt::mpsc::channel::<#event_enum>(#channel_size);
                let (__state_tx, __state_rx) =
                    ::statecraft::__rt::watch::channel(#state_enum::#initial);
                let __shutdown = ::std::sync::Arc::new(::statecraft::__rt::Notify::new());
                let __shutdown_task = ::std::sync::Arc::clone(&__shutdown);

                let __join = ::statecraft::__rt::spawn(async move {
                    let mut __fsm = Self::new(context);
                    loop {
                        ::statecraft::__rt::select! {
                            biased;
                            _ = __shutdown_task.notified() => {
                                // Graceful: drain already-queued events, then stop.
                                while let ::core::result::Result::Ok(__event) =
                                    __rx.try_recv()
                                {
                                    Self::__dispatch(&mut __fsm, __event, &__state_tx).await;
                                }
                                break;
                            }
                            __maybe = __rx.recv() => {
                                match __maybe {
                                    ::core::option::Option::Some(__event) => {
                                        Self::__dispatch(&mut __fsm, __event, &__state_tx).await;
                                    }
                                    // All handles dropped: graceful end.
                                    ::core::option::Option::None => break,
                                }
                            }
                        }
                    }
                });
                let __abort = __join.abort_handle();

                (
                    #handle_name {
                        __tx,
                        __shutdown,
                        __abort,
                        __state_rx,
                    },
                    __join,
                )
            }

            async fn __dispatch(
                fsm: &mut Self,
                event: #event_enum,
                state_tx: &::statecraft::__rt::watch::Sender<#state_enum>,
            ) {
                match fsm.apply(event).await {
                    ::core::result::Result::Ok(()) => {
                        let _ = state_tx.send_replace(fsm.state());
                    }
                    ::core::result::Result::Err(__e) => {
                        ::statecraft::__log_spawn_error(#fsm_name_str, &__e);
                    }
                }
            }
        };

        let types = quote! {
            /// Handle to a spawned FSM. Cheap to clone; use it to send events,
            /// observe state, and control shutdown.
            #[derive(::core::clone::Clone)]
            pub struct #handle_name {
                __tx: ::statecraft::__rt::mpsc::Sender<#event_enum>,
                __shutdown: ::std::sync::Arc<::statecraft::__rt::Notify>,
                __abort: ::statecraft::__rt::AbortHandle,
                __state_rx: ::statecraft::__rt::watch::Receiver<#state_enum>,
            }

            impl #handle_name {
                /// Send an event to the FSM (fire-and-forget). Errors only if the
                /// task has stopped.
                pub async fn send(
                    &self,
                    event: #event_enum,
                ) -> ::core::result::Result<(), ::statecraft::__rt::SendError<#event_enum>> {
                    self.__tx.send(event).await
                }

                /// A `watch` receiver for the current state, updated after each
                /// transition.
                pub fn watch(&self) -> ::statecraft::__rt::watch::Receiver<#state_enum> {
                    self.__state_rx.clone()
                }

                /// Graceful shutdown: process already-queued events, then stop.
                pub fn shutdown(&self) {
                    self.__shutdown.notify_one();
                }

                /// Hard shutdown: abort the task, dropping queued events.
                pub fn shutdown_now(&self) {
                    self.__abort.abort();
                }
            }
        };

        (impl_items, types)
    } else {
        (quote! {}, quote! {})
    };

    Ok(quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum #state_enum {
            #(#states,)*
        }

        #[derive(Debug)]
        pub enum #event_enum {
            #(#event_variants)*
        }

        #(#next_enums)*

        pub struct #fsm_name {
            pub state: #state_enum,
            pub context: #context_ty,
            __queue: ::std::collections::VecDeque<#event_enum>,
        }

        impl #fsm_name {
            pub fn new(context: #context_ty) -> Self {
                Self {
                    state: #state_enum::#initial,
                    context,
                    __queue: ::std::collections::VecDeque::new(),
                }
            }

            pub fn state(&self) -> #state_enum {
                self.state
            }

            /// Enqueue a follow-up event for this FSM. It is processed after the
            /// current handler returns and its transition is applied (deferred,
            /// FIFO). Visibility is controlled by the `public-emit` feature.
            #emit_vis fn emit(&mut self, event: #event_enum) {
                self.__queue.push_back(event);
            }

            /// Drop all pending self-emitted events and enqueue this one as the
            /// sole follow-up. Use when a newly relevant event makes the queued
            /// ones obsolete. Deferred like `emit`; affects only the self-emit
            /// queue, not the spawned event channel. Visibility follows
            /// `public-emit`.
            #emit_vis fn emit_replace(&mut self, event: #event_enum) {
                self.__queue.clear();
                self.__queue.push_back(event);
            }

            pub async fn apply(
                &mut self,
                event: #event_enum,
            ) -> ::core::result::Result<(), ::statecraft::ApplyError<#error_ty>> {
                let __result = self.__drive(event).await;
                if __result.is_err() {
                    // Drop any pending self-emitted events so they do not leak
                    // into a later `apply`.
                    self.__queue.clear();
                }
                __result
            }

            async fn __drive(
                &mut self,
                event: #event_enum,
            ) -> ::core::result::Result<(), ::statecraft::ApplyError<#error_ty>> {
                const __LIMIT: usize =
                    ::statecraft::cascade_limit(::core::option_env!("STATECRAFT_CASCADE_LIMIT"));

                // External event: an undeclared (state, event) pair is an error.
                if self.__apply_one(event).await?.is_some() {
                    return ::core::result::Result::Err(
                        ::statecraft::ApplyError::NoTransition,
                    );
                }

                let mut __steps: usize = 0;
                while let ::core::option::Option::Some(__event) = self.__queue.pop_front() {
                    __steps += 1;
                    if __LIMIT != 0 && __steps > __LIMIT {
                        return ::core::result::Result::Err(
                            ::statecraft::ApplyError::CascadeOverflow,
                        );
                    }
                    // Self-emitted event with no handler here: log and skip.
                    // `__apply_one` hands the event back when unhandled.
                    if let ::core::option::Option::Some(__unhandled) =
                        self.__apply_one(__event).await?
                    {
                        ::statecraft::__unhandled_emit(#fsm_name_str, &self.state, &__unhandled);
                    }
                }
                ::core::result::Result::Ok(())
            }

            // Runs the handler for (state, event). Returns Ok(None) if a handler
            // matched, Ok(Some(event)) if none is declared (the event is handed
            // back so it can be reported without requiring `Event: Copy`);
            // handler errors propagate as Err.
            async fn __apply_one(
                &mut self,
                event: #event_enum,
            ) -> ::core::result::Result<
                ::core::option::Option<#event_enum>,
                ::statecraft::ApplyError<#error_ty>,
            > {
                match (self.state, event) {
                    #(#arms)*
                    (_, __event) => ::core::result::Result::Ok(
                        ::core::option::Option::Some(__event),
                    ),
                }
            }

            #adapter_impl_items

            #(#cleaned)*
        }

        #adapter_types
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

/// A distinct event in the generated event enum: a name and an optional payload
/// type.
struct EventDef {
    name: Ident,
    payload: Option<Type>,
}

/// Record an event from an `#[on]`, enforcing that every occurrence of the same
/// event name declares the same payload (or all are unit).
fn add_event(events: &mut Vec<EventDef>, on: &OnAttr) -> syn::Result<()> {
    let payload_str = |p: &Option<Type>| p.as_ref().map(|t| quote!(#t).to_string());
    if let Some(existing) = events.iter().find(|e| e.name == on.event) {
        if payload_str(&existing.payload) != payload_str(&on.payload) {
            return Err(syn::Error::new_spanned(
                &on.event,
                format!(
                    "event `{}` is declared with inconsistent payloads across #[on] \
                     attributes; every occurrence must use the same payload type",
                    on.event
                ),
            ));
        }
        return Ok(());
    }
    events.push(EventDef {
        name: on.event.clone(),
        payload: on.payload.clone(),
    });
    Ok(())
}
