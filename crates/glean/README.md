# Glean

A terminal client for GitHub pull requests. Run it inside a repository to see
its pull requests, and open one to review it in [Sift](../sift).

Early days: for now `glean` only prints which GitHub repository it would list
pull requests for.

## Usage

```bash
glean
```

Glean looks for the repository on the `upstream` remote first, then on
`origin`, so in a fork it shows the pull requests of the project you forked.

## Requires

Rust stable (edition 2024) and git. Once Glean opens pull requests in Sift, it
will also need Sift checked out next to this repository.
