# Design

The user experience Quorum is being built toward. Decisions here were settled
deliberately; where one was contested, the rejected alternative is named so it
does not get relitigated.

## What Quorum is

Quorum runs parallel coding agents and reviews what they write. Each agent owns
a git worktree. Quorum spawns them, watches them work, and turns finished work
into pull requests.

Review is a first-class surface rather than a side panel, and that is the whole
difference between Quorum and the session managers it otherwise resembles.
Worktrees make it cheap to start five agents; they do nothing to make five
diffs cheap to read.

Rejected: a review surface that happens to read worktrees. Quorum spawns and
drives agents. The reading is what it does better than its neighbours, not what
it is limited to.

## Tabs

An agent is a worktree, one to one. Tabs sit in a row across the top, browser
style, and are switched by cycling or by number.

    ● auth-fix 3   ! dashboard   ✓ perf-pass 12   ○ docs 1   ○ main 4

- `●` working — the agent is writing right now
- `!` blocked — waiting on a permission answer or a question
- `✓` finished and unreviewed
- `○` idle, or no agent attached

`!` is the only state that gets a loud colour. If two things are loud, nothing
is. `✓` is not green: green and red already mean added and removed.

The label comes from ACP's `session_info_update`, which carries a human-readable
title the agent maintains, falling back to the branch name and then to `new`.
The magnitude is a file count. It is not written `+3`, which reads as added
lines in a diff tool.

A `✓` dims once you have looked at that tab since the agent last wrote, and
brightens when it writes again. This lives in memory and never on disk. A
persisted viewed-flag would spend its life being invalidated by an agent
editing files while you read them, and a flag that is usually wrong is worse
than no flag.

**Tabs never reorder.** Sorting by attention means pressing cycle twice lands
somewhere different depending on what happened while you were not looking.

Six tabs is the target. The bar stops working somewhere past that, and the
answer then is a different design, not a smaller font.

`main` gets a tab. Putting complex work on the main worktree is a workaround
for worktrees being hard to review, so Quorum should dissolve the reason for
it — but `main` is where your own uncommitted work lives and it stays visible.

Every existing worktree is adopted on launch, agent or no agent. Quorum has to
be useful in a repo it did not create, and has to review an agent someone
started from a plain terminal.

## Layout

    ┌──────────────────────────────────────────────────────────┐
    │ ● auth-fix 3   ! dashboard   ✓ perf-pass 12   ○ main 4   │
    ├──────────────┬───────────────────────────┬───────────────┤
    │ file tree    │ diff                      │ conversation  │
    │              │                           │               │
    ├──────────────┴───────────────────────────┴───────────────┤
    │ terminal                                                 │
    └──────────────────────────────────────────────────────────┘

Persistent surfaces get columns; transient output gets the bottom drawer.

Breakpoints: below roughly 159 columns the conversation collapses to a status
line and is summoned as an overlay. Below 114 the file tree goes, as it already
does.

Rejected: the conversation in the bottom drawer. Chat is tall-and-narrow
content and reads badly in a short wide slot, and the drawer is where command
output belongs.

The file list is a directory tree with compact folders, so single-child chains
render as one row. It fixes left-truncated paths, which are indistinguishable
on generated names. `Enter` or `Space` toggles a directory; `h` and `l` stay
with the diff.

Terminals are one global list. A new terminal opens in the active tab's
worktree, so long-running processes survive tab switches while new shells still
land in the right place.

## Agents

Agents are driven over ACP, not by running them in a pseudoterminal.

A PTY would give you Claude Code's entire interface for free, and it is what
claude-squad does. It is rejected because `!` blocked is the pivot of the whole
triage loop, and over a PTY that state is a regex against rendered output that
breaks whenever the agent's UI changes. A loud glyph that is sometimes wrong
stops being trusted, and then you are checking five tabs by hand. ACP reports
blocked, running and done as facts, and makes agents interchangeable.

A pseudoterminal is still used, for the shell drawer only.

## Controls

Keys are global and mean one thing everywhere. There is no focus model and no
per-pane keymap: the same key doing different things depending on invisible
state is what makes a TUI need a searchable keybinding menu.

Two states capture keys, both explicit and both visibly different: the prompt
overlay and the shell drawer.

    tabs        cycle, and 1..9
    files       ] [
    diff        j k g G h l
    hunks       } {
    palette     ctrl-t

`Ctrl-T` opens a unified palette in the shape of Arc's command bar: one input
ranking agents, files and commands together, searching across every tab rather
than the current one. When a path matches in more than one worktree it collapses
to one row naming who touched it, which is a collision surfaced for the price of
a row template.

The palette carries every verb Quorum has, including ones with no key, and
displays each shortcut beside its entry. It is what makes the keymap
discoverable instead of another thing to memorise.

Mouse capture stays on. Taking the terminal's native selection means Quorum owes
a better one: selection by line rather than by rectangle, copying the file's
real text rather than what was rendered, and OSC 52 so it reaches the clipboard
over SSH.

## Lifecycle

**Spawn** opens an empty session, the way a fresh agent starts. The worktree is
created lazily at the first prompt, and the agent names the branch from what you
asked for. The base is the default branch and is editable.

Before its first prompt, a tab is only a conversation. There is no worktree, no
file tree and no diff, and nothing has to special-case their absence.

**Arrival** is a summary, not a file. Nothing is selected when a tab opens.

    fix flaky auth session test                   ✓ done · 4m
    ─────────────────────────────────────────────────────────
    5 files   +84  −31

      Rewrote the session fixture to use a fixed clock instead
      of Instant::now(), which was the source of the flake.

    ▸ ⚠ src/git.rs also changed by perf-pass

Three of five tickets are easy: you read a paragraph, check the stat, and ship
without opening a file. It is also where a collision belongs, being the one
thing that is about other tabs. An adopted worktree with no agent gets a thinner
version — stat and tree, no paragraph.

Rejected: ranking files so the most interesting one is selected on arrival. A
heuristic that silently decides what matters hides the file you needed when it
is wrong.

**Follow** is a toggle, off by default. While on, the view tracks whatever the
agent is writing. Any movement turns it off, so reading never fights the cursor.

**Interruptions are scoped to the tab you are in.** A permission request in the
current tab expands the conversation. In any other tab it turns the glyph `!`
and nothing else. Nothing ever steals focus across tabs: with five agents, a
tool that yanks you toward whoever spoke last means you never finish reading
anything.

Rejected: a queue for answering permission requests without switching tabs.
Answering away from the conversation that produced it is how you approve
something you should not have.

**Ship** squashes. Agent work is one ticket and one session; the intermediate
commits are flailing, not history. The agent writes the commit message and PR
body, because it is the only thing in the system that knows why — it has the
conversation, you have the diff. Everything is editable in one form, then
commit, push, `gh pr create`, and the tab closes.

**Discard** is the only dangerous verb, since uncommitted agent work exists in
no commit, no remote and no reflog. Its confirmation names what dies:

    Discard perf-pass?
    12 files changed, never committed. This cannot be undone.

The tab's `×` routes through that confirmation. In every other tabbed
application `×` is reversible, and here it is not.

**Detach the agent** is the third verb and the only non-destructive one. The
session ends, the tab stays, the glyph goes `○`, the diff remains. It is the one
you reach for most, so it is the cheap keystroke.

There is no close. A tab is a worktree; adoption is unconditional, so a closed
tab would return on the next launch. Ship and discard are the only exits, which
keeps the bar honest and self-limiting.

## Dependencies

`gh`, shelled out — not octocrab. octocrab needs 27 required dependencies
against `std::process::Command`, needs a token you would most likely get by
shelling out to `gh auth token` anyway, and answers state queries no better than
`gh pr status --json`. Shelling out was rejected for git because it spawns a
process per operation; opening a pull request does not have that shape. Polling
PR and CI status across five tabs would, and that is when to revisit this.

Stacking is out of scope. `gh stack` exists and does the hard parts, but half
its surface — `up`, `down`, `switch`, `checkout` — works by checking branches
into one directory, which is the opposite of a worktree per agent.

Because stacks are not modelled, spawning from a focused tab bases on the
default branch rather than that tab's branch. Otherwise a pull request carries
its parent's work.

## Open

- Whether the conversation column defaults on or off.
- When collisions are computed. `merge_base_with_graph` is the form to use
  across every agent pair.
- Which terminal emulator backs the shell drawer. `tui-term` wraps `vt100` and
  is compatible with the ratatui already in the lockfile; `alacritty_terminal`
  is more complete and needs a grid-to-buffer bridge written by hand.
- How far selection and copy go, particularly across a side-by-side divider.
