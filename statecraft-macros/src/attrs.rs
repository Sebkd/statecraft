//! Parsing of the `#[on(...)]` attribute and handler signatures.

use syn::{
    Attribute, Ident, Token, Type, bracketed, parenthesized,
    punctuated::Punctuated,
    spanned::Spanned,
    token::{Bracket, Paren},
};

/// Parsed `#[on(state = .., event = .., next = ..)]` attribute.
pub(crate) struct OnAttr {
    pub(crate) state: Ident,
    pub(crate) event: Ident,
    /// Payload type when the event is declared as `event = Foo(Type)`.
    pub(crate) payload: Option<Type>,
    pub(crate) next: Vec<Ident>,
}

pub(crate) fn parse_on(attr: &Attribute) -> syn::Result<OnAttr> {
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

/// Heuristic: a handler is fallible when its return type is a path ending in
/// `Result` (e.g. `Result<T, E>` or `std::result::Result<T, E>`). Type aliases
/// are not resolved.
pub(crate) fn returns_result(sig: &syn::Signature) -> bool {
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
