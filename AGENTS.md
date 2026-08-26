# AGENTS.md

Quorum — a review surface for parallel coding agents. One crate: `src/diff` is
the diff model, `src/git` the git plumbing. The core is UI-agnostic.

## Read first

Code style is @STYLE.md. It is not advisory.

## Commands

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo insta review
```

## Before reporting work as done

All three must pass: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`. If one fails, paste the output — do not describe it, and do not
report the work as complete.

`lefthook.yml` formats and re-stages on `pre-commit`, then runs clippy and
tests — a backstop, not the gate. Once, after cloning:
`brew install lefthook && lefthook install`.

## Dependencies

Ask before adding a crate to `Cargo.toml`.

## Commits

Do not commit unless asked.

When asked: Conventional Commits. A body only when the choice was non-obvious —
say what you rejected and why. There are no ADRs and no rationale in the code,
so that body is the only record.

```
feat(align): pair by similarity, not position

Myers minimises edit count, so on source it anchors hunks on braces.
Histogram anchors on rare lines instead.
```

## Scope

Do what was asked. Do not refactor around the change, rename, or reorganise
unless that was the task.
