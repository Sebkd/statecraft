# Contributor & Agent Guide

This file orients human contributors and coding agents working in the
`statecraft` repository. Read it before making changes.

## What this crate is

`statecraft` turns a Rust `impl` block into an async finite state machine. The
`#[fsm]` attribute macro reads `#[on(state = .., event = .., next = ..)]`
annotations and generates the state/event enums, the FSM struct, and the async
`apply` entry point. Two things matter above all: **branch targets are checked by
the compiler** (each branching transition gets its own target enum), and
**handler logic stays plain, explicit Rust** — no hidden control flow.

## Layout

- `src/` — the runtime facade and core types (`lib.rs`): `ApplyError`, the
  cascade-limit helpers, the `tracing` shims, and the feature-gated `__rt`
  re-exports the generated code relies on. The macro re-export lives here too.
- `statecraft-macros/src/` — the proc-macro, split by responsibility:
  - `lib.rs` — the `#[proc_macro_attribute]` entry point and top-level parsing.
  - `attrs.rs` — parsing of the `#[on(..)]` attribute.
  - `model.rs` — the intermediate model (handlers, events, generated idents).
  - `validation.rs` — compile-time checks that produce user-facing diagnostics.
  - `codegen.rs` — emits the enums, struct, `apply`/`emit`, and Tokio adapter.
- `tests/` — integration tests that exercise runtime behavior.
- `statecraft-macros/tests/` — `trybuild` cases: `pass/` for what must compile,
  `ui/` for compile-fail diagnostics with matching `.stderr` snapshots.
- `examples/` — runnable examples (`axum_fsm/` is a standalone crate).
- `docs/`, `DESIGN.md` — internal working notes. **Both are git-ignored; never
  commit them.** `docs/rules.md` records the workflow (step files, branches, how
  the changelog is kept).

## Everyday commands

- `cargo build --workspace` / `cargo test --workspace` — build and test the core.
- `cargo test --features tokio --test tokio_adapter` — the spawned-mode tests
  (the Tokio adapter changes generated code, so it is tested separately).
- `cargo test -p statecraft-macros` — the `trybuild` UI/pass suite. **Run this
  under default features only**; enabling `tokio`/`public-emit` alters the
  generated code and invalidates the `.stderr` snapshots.
- `cargo clippy --workspace --all-targets -- -D warnings` — lint clean.
- `cargo fmt --check` — formatting.

## How we write code here

Favor plain, readable Rust that matches the surrounding module. Before adding a
new pattern, look at how the nearest code already handles naming, visibility, and
errors, and stay consistent with it.

- Model closed sets (states, events, transition targets) as enums and `match`
  over them exhaustively. Reach for a newtype when it removes a real invalid
  state, not for its own sake.
- Keep the generated code minimal and predictable. When emitting spans, attach
  them to the user's own tokens so diagnostics point at the handler, not at
  macro output — this is the whole point of the validation layer.
- Library code uses typed errors. No `anyhow` outside binaries, examples, or
  tests. Avoid `.unwrap()`/`.expect()` unless the invariant is local and obvious.
- Don't clone or allocate when a borrow will do.
- Prefer an enum over a bare `bool` parameter when it makes the call site read
  better.
- Comments earn their place: non-obvious invariants, generated-code constraints,
  surprising behavior. Don't narrate what the code already says.
- The public API is a contract. Don't break it unless the task says to.

When describing your work, be concrete: state your assumptions, skip the obvious
Rust background, and report what you actually ran.

## Diagnostics

Compile-time checks are a feature, not an afterthought. When you add or change a
check in `validation.rs`, add a `ui/` case with its `.stderr` snapshot and make
sure the message names the offending handler. Regenerate snapshots deliberately
(`TRYBUILD=overwrite`) and read the diff before committing it.

## Changelog, branches, commits

- Branches follow the tracker: `SEBKD-1`, `SEBKD-2`, …
- Conventional Commits: `feat(macros): …`, `fix: …`, `docs: …`, `test: …`,
  `chore: …`. Imperative subjects.
- Every user-visible change updates **both** `CHANGELOG.md` (English, primary)
  and `CHANGELOG_RU.md` (Russian mirror).
- PRs summarize the behavior change, link the issue, and call out any docs,
  example, or feature-flag impact. Include the results of the checks you ran.
