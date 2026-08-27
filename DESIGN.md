# Design

The user experience Quorum is being built toward. Decisions here were settled
deliberately; where one was contested, the rejected alternative is named so it
does not get relitigated.

## What Quorum is

Quorum is a terminal workspace for directing parallel coding agents. It starts
agents in isolated git worktrees, shows which ones need attention, carries the
conversation, gives the human a shell, inspects the work and turns finished work
into pull requests.

The unit is the whole agentic work loop, not the diff. Conversation and changes
share the workspace, with focused Review available when the changes need the
whole body. Review remains first-class because generated work still has to be
understood, but it is a feature of directing agents rather than the frame around
every interaction.

Rejected: a diff viewer with chat attached. A permanent diff makes inactive
files the centre of the interface while an agent is thinking, asking, or
waiting. Quorum gives the current part of the loop the room.

Rejected: a session manager that hands review off to another program. Work,
conversation and changes share a worktree and a history; separating them loses
the context that makes moving between them cheap.

## Projects and workspaces

A Project is a repository. Quorum keeps several Projects open at once, with
their agents continuing to work regardless of which workspace is active.

A workspace holds at most one agent and, after the first prompt, one worktree.
An adopted worktree has a workspace without an attached agent; a new task has a
workspace before its worktree exists.

Projects and workspaces live in a navigator on the left. It replaces
browser-style tabs: Projects are the hierarchy Quorum actually has, and a
vertical navigator can show more than a single row without inventing a second
switcher.

    PROJECTS
    ▾ quorum
      ● auth-fix 3
      ! dashboard
      ✓ perf-pass 12
      ○ main 4
    ▾ kicksite-web
      ! COMMS-434 5
      ○ main

- `●` working — the agent is writing right now
- `!` blocked — waiting on a permission answer or a question
- `✓` finished — the workspace has not been visited since the agent stopped
- `○` idle, or no agent attached

`!` is the only state that gets a loud colour. If two things are loud, nothing
is. `✓` is not green: green and red already mean added and removed.

The label comes from ACP's `session_info_update`, which carries a human-readable
title the agent maintains, falling back to the branch name and then to `new`.
The magnitude is a file count. It is not written `+3`, which reads as added
lines in a diff tool.

A `✓` dims once you have visited that workspace since the agent last wrote, and
brightens when it writes again. This lives in memory and never on disk. A
persisted viewed-flag would spend its life being invalidated by an agent writing
while you work, and a flag that is usually wrong is worse than no flag.

**Projects and workspaces never reorder themselves.** Sorting by attention
means pressing cycle twice lands somewhere different depending on what happened
while you were not looking. The navigator scrolls and Projects collapse; a loud
Project mark preserves attention when its workspaces are hidden.

`main` gets a workspace. It is where your own uncommitted work and manual
terminals live, so hiding it would make Quorum an incomplete view of the
repository.

Every existing worktree is adopted on launch, agent or no agent. Quorum has to
be useful in a repo it did not create, and has to inspect work from an agent
someone started in a plain terminal.

## Layout

On a wide terminal, the Project navigator, conversation and Changes inspector
sit side by side. Conversation is the centre of work rather than a narrow
annotation on a permanent diff.

    ┌────────────────┬──────────────────────┬────────────────────────┐
    │ Projects       │ Work                 │ Changes | src/git.rs    │
    │ and workspaces │                      │                        │
    │                │ conversation         │ selected inspector tab │
    │                │ plan and tools       │                        │
    │                ├──────────────────────┤                        │
    │                │ prompt               │                        │
    ├────────────────┴──────────────────────┴────────────────────────┤
    │ terminal drawer                                               │
    └────────────────────────────────────────────────────────────────┘

The inspector has a pinned `Changes` tab that cannot close. It holds the outcome
summary, changed-file tree, statistics, comments and collisions. Opening a path
creates a document tab. A changed path opens as a diff by default and can switch
to its resulting source; opening the same path again activates its existing tab.
Tool calls and paths in conversation open these tabs directly.

Focused Review expands the same inspector instead of creating another
navigation model. Returning restores conversation and the previous divider
position. Finishing a turn marks the workspace; it never changes the active
inspector tab or enters focused Review underneath the user.

Follow is an inspector toggle, off by default. While on, it tracks the file the
agent is changing. Any manual file movement turns it off. Without Follow, agent
activity marks files but never replaces the selected document or reading
position.

Very wide terminals may keep the changed-file tree beside a selected diff.
Ordinary wide terminals show conversation and the selected inspector tab, with
the tree in `Changes`. At narrower widths the navigator and then the inspector
become explicit overlays or full-body views rather than squeezing every column
until none is useful.

The changed-file tree uses compact folders, so single-child chains render as one
row. It fixes left-truncated paths, which are indistinguishable on generated
names.

Rejected: automatically choosing a view from agent state. An agent finishing
while you read its conversation should not replace it with a diff, and an agent
writing while you review should not pull you back to Work.

Rejected: the conversation in the bottom drawer. Conversation is durable work,
not transient output. The drawer belongs to terminals.

Terminals are one global list. A new terminal opens in the active workspace's
worktree, so long-running processes survive workspace switches while new shells
still land in the right place.

## Agents

Agents are driven over ACP, not by running them in a pseudoterminal.

A PTY would give you Claude Code's entire interface for free, and it is what
claude-squad does. It is rejected because `!` blocked is the pivot of the whole
triage loop, and over a PTY that state is a regex against rendered output that
breaks whenever the agent's UI changes. A loud glyph that is sometimes wrong
stops being trusted, and then you are checking five workspaces by hand. ACP reports
blocked, running and done as facts, and makes agents interchangeable.

A pseudoterminal is still used, for the shell drawer only.

Claude Code is the first supported agent. Quorum still talks to it through ACP
so another agent can be added without making Claude's rendered interface part of
the product.

## Persistence

Quorum has one user-local SQLite database. A Project record represents a
repository; workspaces, transcripts, agent sessions, comments, stack
relationships and outcomes belong to it. The database is never placed in a
repository or synchronized remotely by default.

Workspace history is Quorum's record, independent of whether an ACP session can
resume. On launch Quorum restores Projects and workspaces, rediscovers their
worktrees and attempts to resume Claude where supported. Failure leaves a
detached workspace with its transcript and changes intact.

Several Quorum processes may run at once. Control is leased per workspace: many
processes may inspect one, but only its owner may prompt, answer permissions,
cancel, attach, detach or control its terminals. Stale leases expire; taking a
live lease requires explicit confirmation.

## Skills

Quorum distinguishes three things:

- Quorum application commands change visible application state.
- Portable Quorum skills teach an agent a Quorum-specific workflow.
- Agent-native capabilities are discovered and reported through ACP.

Bundled skills ship as a local Claude Code plugin under the `quorum` namespace.
A skill is an inspectable `SKILL.md` bundle with optional references, templates
and scripts. Loading never executes code; scripts run through Claude's ordinary
tools and permissions. Invocation policy belongs to each skill and defaults to
explicit-only. Quorum may suggest a skill but never silently starts one.

The plugin also registers a fixed local MCP server from the Quorum binary.
Skills use its structured, read-only tools to inspect Projects, workspaces,
transcripts, comments, collisions and Ship facts. It exposes no arbitrary SQL
and no cross-workspace mutation. User-visible Quorum commands retain prompting,
permissions, cancellation, workspace creation, restacking, discard and Ship.

Skills are introduced with the feature they deepen:

- `quorum:help` explains Quorum and its current controls.
- `quorum:address-comments` handles a batch of user feedback.
- `quorum:workspaces` finds and reads current or historical workspace context.
- `quorum:delegate` proposes independent or stacked work for another agent.
- `quorum:resolve-restack` helps resolve conflicted files interactively.
- `quorum:ship-prep` drafts outcome and pull-request prose from the transcript.

Rejected: a general coding skill library. Claude already supplies generic
debugging, review and planning capabilities. A Quorum skill earns its place by
using Quorum context, producing structure Quorum consumes or making a Quorum
workflow safer.

## Controls

Keys are global and mean one thing everywhere. There is no focus model and no
per-pane keymap: the same key doing different things depending on invisible
state is what makes a TUI need a searchable keybinding menu.

Three states capture keys, all explicit and visibly different: the prompt, an
interruption overlay and the shell drawer. Permission choices are selected
inside the interruption overlay.

    workspaces   cycle
    files        ] [
    diff         j k g G h l
    hunks        } {
    palette      ctrl-t

`Ctrl-T` opens a unified palette in the shape of Arc's command bar: one input
ranking Projects, workspaces, files, skills and commands together. When a path
matches in more than one worktree it collapses to one row naming who touched it,
which is a collision surfaced for the price of a row template.

The palette carries every verb Quorum has, including ones with no key, and
displays each shortcut beside its entry. It is what makes the keymap
discoverable instead of another thing to memorise.

Mouse capture stays on. Taking the terminal's native selection means Quorum owes
a better one: selection by line rather than by rectangle, copying the file's
real text rather than what was rendered, and OSC 52 so it reaches the clipboard
over SSH.

## Comments

Comments turn the Changes inspector back into agent input. A user anchors a
draft to a line or range on either side of a diff, writes an instruction and
either sends it now or continues reviewing and sends all drafts as one batch.
The stored comment keeps its path, side, snapshot, selected text and surrounding
context so Quorum can re-anchor it after edits or mark it stale rather than
attach it to the wrong line.

An idle attached agent receives the batch immediately. Otherwise comments queue
until the current turn ends or the user explicitly cancels it. Detached
workspaces keep their queue until the user resumes or starts an agent; writing a
comment never launches one.

Claude may mark a comment Addressed, Blocked or in need of clarification. Only
the user marks it Resolved or Dismissed. Ship always summarizes draft, queued,
addressed, blocked and stale comments and requires an explicit choice when any
remain.

## Delegation

An agent may use `quorum:delegate` to propose another workspace when it finds
work that should be independent or parallel. The proposal names the Project,
base, prompt, expected scope and relationship to its parent. Quorum creates
nothing until the user reviews and approves it.

A proposal may be independent from the Project's default branch or stacked on
its parent. The child receives a compact handoff rather than the parent's whole
transcript, and both workspaces retain a visible relationship in the navigator.
Agents may propose fan-out; only the user grants it.

## Stacks

Quorum models stacked work explicitly. A child knows its parent, the checkpoint
it began from, the changes it owns, whether it needs restacking and the order in
which the stack can land. Creating a child from dirty parent work visibly makes
an intermediate commit so the new worktree has clean Git ancestry.

Parent changes mark descendants as needing Restack; they never rebase a child
under an active agent. Restack is an explicit rebase, runs from parent to child,
uses Git `rerere` and verifies the resulting ancestry. A conflict pauses in the
affected child's existing conversation. `quorum:resolve-restack` can help edit
the resolution, while Quorum retains control of continuing, aborting and
verifying the rebase.

Quorum follows `gh stack` semantics for branch order, one pull request per
workspace, submission, restacking and landing. Its SQLite graph remains the
local source of truth because current `gh stack` operations do not reliably
handle branches checked out across worktrees. Quorum uses `gh stack` where it is
worktree-safe and verifies Git state around every integration.

## Lifecycle

**New task** opens an empty agent workspace on Work, the way a fresh agent
starts. Opening Quorum itself never installs or launches an agent. The worktree
is created lazily at the first prompt, and the agent names the branch from what
you asked for. The base is the default branch and is editable.

Before its first prompt, a workspace is only a conversation. There is no
worktree, Changes content or terminal yet.

**Completion** stays in conversation and updates the pinned `Changes` tab.
Nothing is selected until the user opens it.

    fix flaky auth session test                   ✓ done · 4m
    ─────────────────────────────────────────────────────────
    5 files   +84  −31

      Rewrote the session fixture to use a fixed clock instead
      of Instant::now(), which was the source of the flake.

    ▸ ⚠ src/git.rs also changed by perf-pass

Three of five tickets are easy: you read a paragraph, check the stat, and ship
without opening a file. This is also where a collision belongs, being the one
thing that is about other workspaces. An adopted worktree with no agent gets a
thinner version — stat and tree, no paragraph.

Rejected: ranking files so the most interesting one is selected on arrival. A
heuristic that silently decides what matters hides the file you needed when it
is wrong.

**Interruptions are scoped to the workspace you are in.** A permission request
in the current workspace opens an interruption overlay with its context and
choices. In any other workspace it turns the glyph `!` and nothing else. Nothing
ever steals the active workspace: with five agents, a tool that yanks you toward
whoever spoke last means you never finish anything.

Rejected: a queue for answering permission requests without switching workspaces.
Answering away from the conversation that produced it is how you approve
something you should not have.

**Ship** squashes. Agent work is one ticket and one session; the intermediate
commits are flailing, not history. The agent writes the commit message and PR
body, because it is the only thing in the system that knows why — it has the
conversation, Quorum has the worktree. Everything is editable in one form, then
commit, push, create or update the pull request through `gh` or `gh stack`, and
the workspace closes.

Ship always begins with a preflight. Quorum supplies facts it can verify:
changes, Git state, validation, comments, collisions and stack position.
`quorum:ship-prep` supplies editable explanation, commit and pull-request prose.
The agent never gets to turn an unobserved claim such as "tests pass" into a
green fact.

**Discard** is the only dangerous verb, since uncommitted agent work exists in
no commit, no remote and no reflog. Its confirmation names what dies:

    Discard perf-pass?
    12 files changed, never committed. This cannot be undone.

The workspace's remove action routes through that confirmation rather than
borrowing the reversible `×` of an ordinary document tab.

**Detach the agent** is the third verb and the only non-destructive one. The
session ends, the workspace stays, the glyph goes `○`, and the worktree remains.
It is the one you reach for most, so it is the cheap keystroke.

An empty task with no worktree can close. A workspace with a worktree cannot:
adopted worktrees would return on the next launch, and hiding one would make the
navigator lie about the repository. Ship and discard are an agent worktree's
exits. The main worktree is permanent and neither verb removes it.

## Dependencies

`gh`, shelled out — not octocrab. octocrab needs 27 required dependencies
against `std::process::Command`, needs a token you would most likely get by
shelling out to `gh auth token` anyway, and answers state queries no better than
`gh pr status --json`. Shelling out was rejected for git because it spawns a
process per operation; opening a pull request does not have that shape. Polling
PR and CI status across five workspaces would, and that is when to revisit this.

`gh stack` is the preferred vocabulary and remote integration for stacked pull
requests, but not the local source of truth until it safely supports branches
checked out in separate worktrees. Rebase is an infrequent lifecycle operation,
so invoking Git for it is different from shelling out once per file or status
query.

## Roadmap

### 1. Project and workspace foundation

- Add the user-local SQLite database and migrations.
- Model Projects, persistent workspaces, transcripts and ownership leases.
- Open several Projects at once.
- Replace workspace tabs with the Project navigator.
- Route every event by workspace identity.
- Restore detached workspaces after restart.
- Establish the command catalog and unified palette.

This phase ends when Quorum can represent several repositories and all of their
existing worktrees reliably, even without active agents.

### 2. Daily agent loop

- Make conversation the centre of the workspace.
- Add the side-by-side Changes inspector and its document tabs.
- Create tasks and worktrees lazily.
- Start, resume, cancel and detach Claude.
- Make agent state, permissions, history, plans and tool activity usable.
- Add the terminal drawer.
- Ship `quorum:help`.

This is the first usable milestone: direct several agents, intervene, inspect
their files, use a shell, quit and return without losing the work.

### 3. Changes and feedback

- Preserve selected files and reading positions through refreshes.
- Connect conversation paths and tool calls to document tabs.
- Add source and diff views with Follow off by default.
- Add anchored comments, drafts, batches and the idle queue.
- Persist comment outcomes and warn about them during Ship.
- Ship `quorum:address-comments` and its read-only tools.

This phase ends when a user can inspect work and direct exact fixes without
translating file locations back into prose.

### 4. Multi-agent coordination

- Ship `quorum:workspaces` and cross-workspace read tools.
- Surface changed-path overlap and collisions.
- Add structured delegation proposals and user-approved workspace creation.
- Record parent, child and handoff relationships.
- Ship `quorum:delegate`.
- Search across Projects and recover detached work from persisted context.

This phase ends when agents and users can understand related work, split tasks
and reuse prior context safely.

### 5. Stacks and landing

- Create intermediate checkpoints for stacked work.
- Add the stack graph, explicit Restack and `rerere`.
- Ship the interactive `quorum:resolve-restack` flow.
- Integrate worktree-safe `gh stack` operations and verify Git ancestry.
- Submit and land stacked pull requests.
- Add Ship preflight, `quorum:ship-prep`, detach and discard.

This phase ends when Quorum can carry delegated work from its first checkpoint
through a safely landed stack.

### 6. Refinement

- Tune responsive breakpoints, dividers and inspector tab retention.
- Add line-oriented selection and OSC 52 copy.
- Deepen terminal management, notifications and usage controls.
- Improve comment re-anchoring.
- Add more ACP agents when the Claude workflow is proven.
- Explore remote Projects and extension beyond bundled skills.

Details such as schema shape, exact keys, lease timing, comment matching and
individual `gh stack` commands are decided when their phase begins rather than
front-loaded into the product design.

## Open

- When collisions are computed. `merge_base_with_graph` is the form to use
  across every agent pair.
- Which terminal emulator backs the shell drawer. `tui-term` wraps `vt100` and
  is compatible with the ratatui already in the lockfile; `alacritty_terminal`
  is more complete and needs a grid-to-buffer bridge written by hand.
- How far selection and copy go, particularly across a side-by-side divider.
- How long completed Project and workspace history is retained.
- Which inspector document tabs survive a restart.
