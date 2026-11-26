# Sift

Two terminal tools for reading code changes:

- **[Sift](crates/sift)** — a diff viewer over git: side-by-side or unified,
  with syntax highlighting, intraline emphasis, and a file tree.
- **[Glean](crates/glean)** — a client for GitHub pull requests: lists them as
  GitHub's pages do, and opens one in Sift to review.

```bash
cargo install --path crates/sift
cargo install --path crates/glean
```

## Requires

Rust stable (edition 2024) and git.
