# Style

Rust, for Sift. Terseness belongs in prose, not in identifiers.

## Comments

One `//!` line per module, saying what the module is. Never more.

`///` on a type when its name cannot carry the concept. A line or two:

```rust
/// A byte range within a single line, relative to that line's start.
pub struct Span { pub start: usize, pub end: usize }
```

Nothing on fields, methods, or functions. If one needs explaining, rename it or
split it. No `//` anywhere.

Rationale is not code. It goes in the commit body when the choice is
non-obvious.

## Names

Full words. No abbreviations — not `idx`, `ctx`, `cfg`, `buf`; not `o` and `n`
for `old` and `new`. Domain terms are fine when they are the real name of the
thing: `dice`, `bigram`, `histogram`.

## Functions

One job each. No line limit — a long function that does one thing is fine; a
short one that does three is not.

When extracting would mean threading four or more parameters through a helper,
that is the signal to introduce a struct and let its fields be the shared
state.

## Types

A newtype must earn its keep with behaviour — methods, constants, or an
invariant. `Similarity` earns one: bounded, with named values, and comparison
is its whole job. A bare tag over `usize` does not. No type aliases for
primitives either; an alias is a comment that compiles.

Fields are private when they must agree with each other, public when the struct
is passive data (C-STRUCT-PRIVATE):

```rust
pub struct Source { text: String, lines: Vec<Range<usize>>, final_newline: bool }
pub struct Span { pub start: usize, pub end: usize }
```

Derive the full common set — `Clone, Copy, Debug, Default, PartialEq, Eq, Hash`
— dropping only what cannot hold (C-COMMON-TRAITS). A type holding an `f32` has
`PartialEq` and not `Eq`; do not contort the representation to satisfy a derive.

`#[must_use]` only where discarding the result is almost certainly a bug. Not on
getters.

## Modules

`src/thing.rs` beside `src/thing/`, never `mod.rs`. Submodules are private and
the parent re-exports the public surface, so each type has exactly one path:

```rust
// src/diff.rs
mod align;
mod flow;
mod model;

pub use align::{Align, Cell, Row, RowKind, Span, Strategy};
```

## APIs

A layer with tunables is a `Default` struct whose work is a method. Callers tune
with struct-update syntax, so adding a knob never changes a signature.

```rust
let rows = Align::default().rows(&diff);
let rows = Align { strategy: Strategy::Positional, ..Default::default() }.rows(&diff);
```

## Errors

Each module owns a `thiserror` enum and a `pub type Result<T>`. Bad input from
outside is a `Result`. A broken internal invariant panics — `expect` with the
invariant as its message, never `unwrap`.

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

## Performance

Write the idiomatic form. Depart from it only with a benchmark that justifies
the departure. Indices and sizes are `usize`; a narrower integer to save bytes
buys nothing you have measured, and costs a cast at every boundary.

## Tests

All tests live in `tests/` and drive the public API. `src/` contains no
`#[cfg(test)]`. Nothing is `pub` merely to be tested — if a function cannot be
reached through the public surface, it is dead.

Names are sentences stating what is true. No comments in a test body: if a test
needs one, the name is wrong.

```rust
#[test]
fn an_inserted_line_does_not_shift_every_pairing_below_it() {
    insta::assert_snapshot!(table(OLD, NEW));
}
```

Snapshots are the assertion. Read the snapshot diff; never blind-accept with
`cargo insta accept`.

## Suppressions

`#[expect(lint, reason = "...")]`, never `#[allow]`. It warns when the lint stops
firing, so dead suppressions cannot accumulate. The reason records the invariant,
which is why it is not a comment by another name.

```rust
#[expect(clippy::cast_precision_loss, reason = "shared <= bigram count")]
```
