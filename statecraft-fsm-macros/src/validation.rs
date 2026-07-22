//! Parse-time validation of discovered handlers, so common mistakes get a clear
//! message pointing at the handler instead of a cryptic error in generated code.

use std::collections::HashMap;

use quote::quote;

use crate::model::{Handler, next_enum_ident};

/// Validate the handlers. `error_declared` is whether the impl declared
/// `type Error`.
pub(crate) fn check(handlers: &[Handler], error_declared: bool) -> syn::Result<()> {
    // BL-13: two distinct branching transitions whose generated per-transition
    // target enums collide by name (e.g. `(AB, C)` and `(A, BC)` both yield
    // `ABCNext`).
    let mut next_names: HashMap<String, (String, String)> = HashMap::new();
    for h in handlers.iter().filter(|h| h.on.next.len() > 1) {
        let name = next_enum_ident(&h.on).to_string();
        let key = (h.on.state.to_string(), h.on.event.to_string());
        match next_names.get(&name) {
            Some(existing) if *existing != key => {
                return Err(syn::Error::new(
                    h.span,
                    format!(
                        "generated target enum `{name}` collides between transitions \
                         `({}, {})` and `({}, {})`; rename a state or event",
                        existing.0, existing.1, key.0, key.1,
                    ),
                ));
            }
            _ => {
                next_names.insert(name, key);
            }
        }
    }

    for h in handlers {
        // BL-7: a payload event whose handler takes no payload argument.
        if let Some(ty) = &h.on.payload
            && !h.has_payload_param
        {
            let ty_str = quote!(#ty).to_string();
            return Err(syn::Error::new(
                h.span,
                format!(
                    "handler `{}` handles an event with a payload (`event = {}({})`) \
                     but takes no payload argument; add a `{}` parameter, e.g. \
                     `async fn {}(&mut self, _: {})`",
                    h.method, h.on.event, ty_str, ty_str, h.method, ty_str,
                ),
            ));
        }
    }

    // BL-4: a fallible handler but no `type Error` declared (it would default to
    // `Infallible`, giving a cryptic conversion error in generated code).
    if !error_declared && let Some(h) = handlers.iter().find(|h| h.fallible) {
        return Err(syn::Error::new(
            h.span,
            format!(
                "handler `{}` returns `Result`, but the FSM declares no error type; \
                 add `type Error = ...;` to the impl",
                h.method,
            ),
        ));
    }

    Ok(())
}
