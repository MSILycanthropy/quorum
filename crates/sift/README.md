# Sift

A terminal diff viewer over git. It shows side-by-side or unified diffs with
syntax highlighting and intraline emphasis, and a file tree next to them.

## Usage

Run it inside a repository:

```bash
sift                  # working tree against HEAD
sift --staged         # what's staged, against HEAD
sift main             # working tree against main
sift main..feature    # main against feature
sift main...feature   # what feature did since it branched off main
sift --show HEAD~2    # one commit against its parent
sift -- src/          # only paths under src/
```

`sift --no-index old.rs new.rs` compares two files outside a repository.

## Keys

Press `?` for every key. The status line shows the ones you'll use most. These
are the main ones:

```
n / N       next / previous hunk, carrying on into the next file
] / [       next / previous file
/           find a file by name
:  ctrl-p   run any command by name
e           open $EDITOR at the cursor's line
y  Y        copy the file's path, or path:line at the cursor
s           side by side or unified
w           wrap long lines
ctrl-b  f   show or hide the file list
r           reload from git
q           quit
```

`j`/`k` move the cursor a line, `ctrl-d`/`ctrl-u` and the page keys move it and
the view together, and `g`/`G` jump to either end. Side by side, `H`/`L` or
`shift-←`/`shift-→` put the cursor on the old or new side. `h`/`l` and `←`/`→`
scroll sideways.
After `e` the diff reloads, so you see your edit straight away.

## Layout

    src/command.rs   every command and the keys bound to it
    src/diff         diff model, side-by-side alignment, wrapping
    src/git.rs       what changed between a revision and the worktree, index or another revision
    src/ui           the file list, the diff, and the help and palette overlays

The key catalog in `command.rs` is the one place a key gets its meaning. The
help overlay, the palette and the status line hints are all drawn from it, so
they can't drift apart. A program that embeds sift adds its own keys to the
same catalog through `Opening::bindings`, so they show up in all three, and a
key that one of sift's commands already uses is refused.

## Requires

Rust stable (edition 2024) and git ≥ 2.38.
