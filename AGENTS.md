# AGENTS.md

A Cargo workspace of two terminal tools, each with its own `AGENTS.md`, and the
crates they share:

- `crates/sift` — Sift, a terminal diff viewer over git.
- `crates/glean` — Glean, a terminal client for GitHub pull requests, built on Sift.
- `crates/hue` — `Rgb`, and the palette a terminal reports of its own theme.
- `crates/host` — the terminal a TUI runs in: raw mode, the alternate screen,
  input on a thread, the clipboard, and giving it all back on exit or panic.
- `crates/config` — settings files: where they live, and reading them into each
  tool's own types, so Glean's file can hold Sift's settings.
- `crates/keys` — key maps: commands, the key sequences that reach them, what a
  key press amounts to, and rebinding them from a settings file.
- `crates/hints` — the footer along the bottom of a screen: status on the left,
  key hints on the right, read from a key map or given by hand.
- `crates/picker` — a box to type in over a list to pick from, for commands,
  files, or anything else a program can name.
- `crates/settings` — a settings page any of them can show: every setting and
  every key, changed in place and handed back for the program to apply.

Sift stays a pure git tool: nothing about GitHub goes into Sift. Glean depends
on Sift as a library, never the other way round.

## Read first

Code style is @STYLE.md. It is not advisory.

## Commands

From the root, for every crate:

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo insta review
```

`-p sift` or `-p glean` narrows any of them to one crate.

## Before reporting work as done

All three must pass: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`. If one fails, paste the output — do not describe it, and do not
report the work as complete.

`lefthook.yml` formats and re-stages on `pre-commit`, then runs clippy and
tests — a backstop, not the gate. Once, after cloning:
`brew install lefthook && lefthook install`.

## Dependencies

Ask before adding a crate to any `Cargo.toml`. A dependency more than one crate
uses goes in `[workspace.dependencies]` at the root, so its version is chosen
once.

## Commits

Do not commit unless asked.

When asked: Conventional Commits, scoped by crate when a change touches only
one (`feat(glean): ...`, `fix(sift/theme): ...`). A body only when the choice
was non-obvious — say what you rejected and why. There are no ADRs and no
rationale in the code, so that body is the only record.

```
feat(sift/align): pair by similarity, not position

Myers minimises edit count, so on source it anchors hunks on braces.
Histogram anchors on rare lines instead.
```

## Scope

Do what was asked. Do not refactor around the change, rename, or reorganise
unless that was the task.
