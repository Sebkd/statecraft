# Repository Guidelines

## Project Structure & Module Organization
`statecraft` is a Rust workspace (edition 2024) with two crates. Runtime types
live in `src/` (`lib.rs` and future `core.rs`), while the proc-macro
implementation lives in `statecraft-macros/src/`, with parsing, validation, and
code generation split by responsibility. Integration tests belong in `tests/`.
Compile-fail macro coverage should use `statecraft-macros/tests/ui/` with a
`trybuild` harness. Runnable examples belong in `examples/`.

Internal working notes live in `docs/` and `DESIGN.md`; both are git-ignored and
must not be committed. See `docs/rules.md` for how steps, branches, and the
changelog are tracked.

## Build, Test, and Development Commands
- `cargo build --workspace --all-features` builds the whole workspace.
- `cargo test --workspace --all-features` runs all tests.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` lints.
- `cargo fmt` / `cargo fmt --check` apply or verify formatting.

## Coding Style & Naming Conventions
This repo targets Rust 2024. Use `snake_case` for modules, files, functions, and
test helpers. Use `CamelCase` for types, enums, and generated FSM names such as
`WorkerFsm` and `WorkerFsmState`. Keep module boundaries aligned with
responsibility, especially inside `statecraft-macros/src/codegen/` and
`statecraft-macros/src/validation/`.

## Rust Implementation Standards
When writing or refactoring Rust code in this repository, prefer simple,
idiomatic Rust over clever abstractions. Match the existing module boundaries,
naming, visibility, and error handling style before introducing a new pattern.

- Use the type system to encode domain invariants when it removes real invalid
  states or clarifies core FSM logic.
- Prefer enums, newtypes, and smart constructors for meaningful domain concepts
  such as states, events, IDs, validated values, modes, and capabilities.
- Avoid typestate, generics, traits, macros, or phantom types unless they
  clearly simplify the model or prevent bugs the current API can realistically
  allow.
- Prefer typed errors in library code. Avoid `anyhow` outside binaries,
  examples, or tests.
- Avoid `.unwrap()` and `.expect()` in library code unless the invariant is
  local, obvious, and cannot be expressed cleanly in the type system.
- Avoid unnecessary clones, allocations, trait objects, and string conversions.
  Borrow when ownership is not needed.
- Prefer exhaustive `match` statements for closed sets of states, events, and
  return kinds.
- Avoid boolean parameters when an enum would make the call site clearer.
- Keep functions small enough to read linearly and keep abstraction levels
  consistent within each function.
- Keep comments sparse. Add comments only for non-obvious invariants, safety
  reasoning, generated-code constraints, or surprising behavior.
- Do not add comments that merely restate the code.
- Preserve public API compatibility unless the task explicitly allows breaking
  changes.

When explaining work, be concrete. State assumptions explicitly, avoid
speculation, avoid em dashes, and do not over-explain obvious Rust basics.

## Testing Guidelines
Add or update integration tests in `tests/` for runtime behavior and lifecycle
changes. Add UI cases in `statecraft-macros/tests/ui/` for compile-time
diagnostics, with matching `.stderr` files. Prefer descriptive test names
beginning with `test_`. Run workspace tests before opening a PR.

## Commit & Pull Request Guidelines
Use Conventional Commits: `fix(macros): ...`, `docs: ...`, `test: ...`,
`chore: ...`. Keep subjects imperative and scoped when useful. Branches are
named after the tracker issue: `SEBKD-1`, `SEBKD-2`, and so on. Update
`CHANGELOG.md` (`## [Unreleased]`) for user-visible changes. PRs should
summarize the behavior change, link the issue, and call out docs, example, or
feature-flag impact. Include command results for the checks you ran.
