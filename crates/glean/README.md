# Glean

A terminal client for GitHub pull requests. Run it inside a repository to see
its pull requests, and open one to review it in [Sift](../sift).

Early days: for now `glean` prints the repository's pull requests rather than
opening a screen.

## Usage

```bash
glean                            # open pull requests
glean --filter mine              # open, and yours
glean --filter review-requested  # waiting on your review
glean --filter drafts
glean --filter closed            # closed and merged
glean MSILycanthropy/sift        # another repository, from anywhere
```

Glean signs in with `GH_TOKEN` or `GITHUB_TOKEN` when either is set, and
otherwise with the token `gh auth token` prints.

Glean looks for the repository on the `upstream` remote first, then on
`origin`, so in a fork it shows the pull requests of the project you forked.

## Requires

Rust stable (edition 2024) and git. Sift comes with it: both live in this
workspace.
