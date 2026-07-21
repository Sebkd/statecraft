//! Parse-time validation of discovered handlers, so common mistakes get a clear
//! message pointing at the handler instead of a cryptic error in generated code.

use quote::quote;

use crate::model::Handler;

/// Validate the handlers. `error_declared` is whether the impl declared
/// `type Error`.
pub(crate) fn check(handlers: &[Handler], error_declared: bool) -> syn::Result<()> {
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
