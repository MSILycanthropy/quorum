# Quorum

A review surface for parallel coding agents.

Agents generate faster than anyone can review. Quorum is the other half: see what
each agent changed in its worktree, catch collisions before they land, and decide
what merges.

## What it does

- **Worktree diffs** — side-by-side, anchored on each agent's base commit rather
  than a commit range, because agent work is usually still uncommitted.
- **Collision prediction** — `git merge-tree` across agent pairs, so you find out
  two agents are headed for the same lines before either finishes.
- **Sessions** — agents driven over [ACP](https://agentclientprotocol.com), so
  Claude Code, Codex and Gemini are interchangeable.

Terminal first. The core is UI-agnostic; a GUI can come later without touching it.

## Layout

    crates/quorum-git    worktree discovery, git plumbing
    crates/quorum-diff   diff model            (planned)
    crates/quorum-tui    terminal frontend     (planned)

## Requires

Rust stable (edition 2024) and git ≥ 2.38.

## Status

Early. Nothing works yet.
