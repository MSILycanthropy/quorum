# Quorum

A terminal workspace for parallel coding agents.

Quorum keeps the whole agentic work loop in one place: start agents in isolated
git worktrees, see who needs attention, converse and intervene, inspect what they
changed, run commands yourself, and decide what happens to the result.

Review is a first-class feature, not the frame around the application. Work and
Review are explicit surfaces within each agent workspace, with a shell drawer
available when the work needs a human hand.

## What it does

- **Agent workspaces** — one stable tab per agent and worktree, including
  worktrees Quorum did not create.
- **Attention triage** — working, blocked and finished agents stay visible
  without pulling you away from the tab you are using.
- **Interchangeable agents** — sessions are driven over
  [ACP](https://agentclientprotocol.com), rather than scraping a rendered TUI.
- **Integrated review** — worktree diffs are anchored on each agent's base
  commit because agent work is usually still uncommitted.
- **Human intervention** — a shell drawer opens in the active worktree and its
  processes survive tab switches.
- **Cross-agent awareness** — overlapping work is surfaced before agents try to
  land incompatible changes.

Terminal first. The core is UI-agnostic; a GUI can come later without touching it.

## Layout

One crate for now; the seams below are module boundaries, not crate
boundaries, and get split out when they earn it.

    src/agent    ACP sessions and transcripts
    src/diff     diff model, side-by-side alignment, wrapping
    src/git      worktree discovery and git plumbing
    src/ui       Work, Review and shell surfaces

## Requires

Rust stable (edition 2024) and git ≥ 2.38.

## Status

Early. The current build can drive one ACP agent and inspect one worktree with a
side-by-side or unified diff. The multi-worktree application shell, explicit
Work and Review surfaces, shell drawer, lifecycle verbs and cross-agent
awareness described in `DESIGN.md` are still being built.
