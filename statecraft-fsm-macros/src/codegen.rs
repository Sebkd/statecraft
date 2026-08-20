//! Code generation: turn the parsed `impl` block into the generated state/event
//! enums, the FSM struct, `apply`/`emit`, and the optional Tokio adapter.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{FnArg, Ident, ImplItem, ItemImpl, Type, spanned::Spanned};

use crate::attrs::{parse_on, returns_result};
use crate::model::{EventDef, Handler, add_event, add_unique, next_enum_ident};
use crate::validation;

pub(crate) fn expand(
    initial: &Ident,
    channel_size: usize,
    input: &ItemImpl,
) -> syn::Result<TokenStream2> {
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
    let declared_error_ty: Option<Type> = input.items.iter().find_map(|item| match item {
        ImplItem::Type(t) if t.ident == "Error" => Some(t.ty.clone()),
        _ => None,
    });
    let error_declared = declared_error_ty.is_some();
    let error_ty: Type =
        declared_error_ty.unwrap_or_else(|| syn::parse_quote!(::core::convert::Infallible));

    let mut handlers = Vec::new();
    for item in &input.items {
        if let ImplItem::Fn(f) = item
            && let Some(attr) = f.attrs.iter().find(|a| a.path().is_ident("on"))
        {
            handlers.push(Handler {
                method: f.sig.ident.clone(),
                span: f.sig.ident.span(),
                has_payload_param: f.sig.inputs.iter().any(|a| matches!(a, FnArg::Typed(_))),
                on: parse_on(attr)?,
                fallible: returns_result(&f.sig),
            });
        }
    }

    // Clear diagnostics for common handler mistakes (payload arity, missing
    // `type Error`) before generating code.
    validation::check(&handlers, error_declared)?;

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

    // Wholesale heap-placement policy (feature `boxed-all`, forwarded from the
    // facade). Evaluated when this macro crate is compiled; per-transition
    // `#[on(.., boxed)]` marks stay honoured either way.
    let boxed_all = cfg!(feature = "boxed-all");

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
        // Invoking the handler. `boxed` (per transition) or the `boxed-all`
        // feature (every transition) puts the handler's future on the heap, so
        // the arm holds a pointer instead of the whole future. Without it the
        // future is inlined into the dispatch coroutine, and since the
        // coroutine is sized by its largest arm, one heavy handler sets the
        // stack cost of the entire machine.
        let invoke = if h.on.boxed || boxed_all {
            quote_spanned! {h.span=> ::std::boxed::Box::pin(self.#method(#call_arg)) }
        } else {
            quote_spanned! {h.span=> self.#method(#call_arg) }
        };
        // `<invoke>.await`. Fallible handlers wrap the error into
        // `ApplyError::Handler` explicitly (not via `?`/`From`), spanned at the
        // handler, so a wrong error type reads as a clear `mismatched types`
        // there — expected `type Error`, found the handler's error.
        let call = if h.fallible {
            quote_spanned! {h.span=>
                match #invoke.await {
                    ::core::result::Result::Ok(__v) => __v,
                    ::core::result::Result::Err(__e) => {
                        return ::core::result::Result::Err(
                            ::statecraft_fsm::ApplyError::Handler(__e),
                        );
                    }
                }
            }
        } else {
            quote! { #invoke.await }
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
    // Diagnostics (feature `diagnostics`, forwarded from the facade): lets a
    // consumer assert in CI that the dispatch frame stays bounded, instead of
    // discovering a regression as a stack overflow in production.
    let diagnostics_items = if cfg!(feature = "diagnostics") {
        quote! {
            /// Size in bytes of the future returned by [`Self::apply`], for the
            /// given event.
            ///
            /// The future is created and dropped without ever being polled, so
            /// no handler runs and nothing observable happens. Available with
            /// the `diagnostics` feature.
            ///
            /// The value is set by the largest handler inlined into dispatch —
            /// see the `boxed` mark on `#[on]` and the `boxed-all` feature for
            /// keeping it bounded.
            pub fn apply_future_size(&mut self, event: #event_enum) -> usize {
                ::core::mem::size_of_val(&self.apply(event))
            }
        }
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
            ) -> (#handle_name, ::statecraft_fsm::__rt::JoinHandle<()>)
            where
                #context_ty: ::core::marker::Send + 'static,
                #event_enum: ::core::marker::Send + 'static,
            {
                let (__tx, mut __rx) =
                    ::statecraft_fsm::__rt::mpsc::channel::<#event_enum>(#channel_size);
                let (__state_tx, __state_rx) =
                    ::statecraft_fsm::__rt::watch::channel(#state_enum::#initial);
                let __shutdown = ::std::sync::Arc::new(::statecraft_fsm::__rt::Notify::new());
                let __shutdown_task = ::std::sync::Arc::clone(&__shutdown);

                let __join = ::statecraft_fsm::__rt::spawn(async move {
                    let mut __fsm = Self::new(context);
                    loop {
                        ::statecraft_fsm::__rt::select! {
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
                state_tx: &::statecraft_fsm::__rt::watch::Sender<#state_enum>,
            ) {
                match fsm.apply(event).await {
                    ::core::result::Result::Ok(()) => {
                        let _ = state_tx.send_replace(fsm.state());
                    }
                    ::core::result::Result::Err(__e) => {
                        ::statecraft_fsm::__log_spawn_error(#fsm_name_str, &__e);
                    }
                }
            }
        };

        let types = quote! {
            /// Handle to a spawned FSM. Cheap to clone; use it to send events,
            /// observe state, and control shutdown.
            #[derive(::core::clone::Clone)]
            pub struct #handle_name {
                __tx: ::statecraft_fsm::__rt::mpsc::Sender<#event_enum>,
                __shutdown: ::std::sync::Arc<::statecraft_fsm::__rt::Notify>,
                __abort: ::statecraft_fsm::__rt::AbortHandle,
                __state_rx: ::statecraft_fsm::__rt::watch::Receiver<#state_enum>,
            }

            impl #handle_name {
                /// Send an event to the FSM (fire-and-forget). Errors only if the
                /// task has stopped.
                pub async fn send(
                    &self,
                    event: #event_enum,
                ) -> ::core::result::Result<(), ::statecraft_fsm::__rt::SendError<#event_enum>> {
                    self.__tx.send(event).await
                }

                /// A `watch` receiver for the current state, updated after each
                /// transition.
                pub fn watch(&self) -> ::statecraft_fsm::__rt::watch::Receiver<#state_enum> {
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
            ) -> ::core::result::Result<(), ::statecraft_fsm::ApplyError<#error_ty>> {
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
            ) -> ::core::result::Result<(), ::statecraft_fsm::ApplyError<#error_ty>> {
                const __LIMIT: usize =
                    ::statecraft_fsm::cascade_limit(::core::option_env!("STATECRAFT_CASCADE_LIMIT"));

                // External event: an undeclared (state, event) pair is an error.
                if self.__apply_one(event).await?.is_some() {
                    return ::core::result::Result::Err(
                        ::statecraft_fsm::ApplyError::NoTransition,
                    );
                }

                let mut __steps: usize = 0;
                while let ::core::option::Option::Some(__event) = self.__queue.pop_front() {
                    __steps += 1;
                    if __LIMIT != 0 && __steps > __LIMIT {
                        return ::core::result::Result::Err(
                            ::statecraft_fsm::ApplyError::CascadeOverflow,
                        );
                    }
                    // Self-emitted event with no handler here: log and skip.
                    // `__apply_one` hands the event back when unhandled.
                    if let ::core::option::Option::Some(__unhandled) =
                        self.__apply_one(__event).await?
                    {
                        ::statecraft_fsm::__unhandled_emit(#fsm_name_str, &self.state, &__unhandled);
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
                ::statecraft_fsm::ApplyError<#error_ty>,
            > {
                match (self.state, event) {
                    #(#arms)*
                    (_, __event) => ::core::result::Result::Ok(
                        ::core::option::Option::Some(__event),
                    ),
                }
            }

            #adapter_impl_items

            #diagnostics_items

            #(#cleaned)*
        }

        #adapter_types
    })
}
