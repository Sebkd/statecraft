//! The discovered FSM model (handlers, events) and small collection helpers,
//! including the event-payload consistency check.

use quote::{format_ident, quote};
use syn::{Ident, Type};

use crate::attrs::OnAttr;

/// One discovered handler: the method name, its `#[on]` metadata, and whether
/// it returns a `Result` (and thus may fail).
pub(crate) struct Handler {
    pub(crate) method: Ident,
    pub(crate) on: OnAttr,
    pub(crate) fallible: bool,
}

/// A distinct event in the generated event enum: a name and an optional payload
/// type.
pub(crate) struct EventDef {
    pub(crate) name: Ident,
    pub(crate) payload: Option<Type>,
}

/// Name of the generated per-transition target enum for a branching handler.
pub(crate) fn next_enum_ident(on: &OnAttr) -> Ident {
    format_ident!("{}{}Next", on.state, on.event)
}

/// Push `id` onto `list` unless it is already present (first-seen order).
pub(crate) fn add_unique(list: &mut Vec<Ident>, id: &Ident) {
    if !list.iter().any(|existing| existing == id) {
        list.push(id.clone());
    }
}

/// Record an event from an `#[on]`, enforcing that every occurrence of the same
/// event name declares the same payload (or all are unit).
pub(crate) fn add_event(events: &mut Vec<EventDef>, on: &OnAttr) -> syn::Result<()> {
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
