# AGENTS.md

Glean — a terminal client for GitHub pull requests. It lists the pull requests
of the repository you're in, and opens one in Sift, the diff viewer it is built
on, to review, comment, approve and merge.

`src/model` is what Glean shows, in its own terms: the repository on the
forge, pull requests, threads, comments, reviews, checks.

Screens use the model and nothing else. A forge's API types never leave the
module that talks to that forge: it converts them into the model. GitHub is the
only forge for now; Bitbucket may follow, and this is what keeps that a second
converter rather than a rewrite. There is no forge trait until a second forge
exists.

Sift is a library dependency (`../sift`). Sift stays a pure git tool: nothing
about GitHub goes into Sift. Glean talks to Sift through `sift::ui::Surface`,
its annotations and its host bindings. Glean's `ratatui` must be the same
version as Sift's, or their types will not line up.

GitHub is reached over its GraphQL API directly, not by running `gh`. The one
thing taken from `gh` is the token, from `gh auth token`.

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
tests — a backstop, not the gate. Once, after cloning: `lefthook install`.

## Tests

Tests never reach GitHub. Anything that talks to the network takes its
responses from fixtures.

## Dependencies

Ask before adding a crate to `Cargo.toml`.

## Commits

Do not commit unless asked.

When asked: Conventional Commits. A body only when the choice was non-obvious —
say what you rejected and why. There are no ADRs and no rationale in the code,
so that body is the only record.

## Scope

Do what was asked. Do not refactor around the change, rename, or reorganise
unless that was the task.
