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

The status line along the bottom shows the keys you'll want most. Every key is
on the Keys tab of the settings page (`C-,` or `:settings`), where you can
change them too.

Sift takes vim's keys and the ones you'd reach for with arrows and Ctrl, so
most commands have both: `j` and `↓`, `gg` and `Home`, `]c` and `F7`. These
are the main ones:

```
n  ]c  F7       next hunk, carrying on into the next file (N  [c  S-F7 back)
Tab  ]f         next file (S-Tab  [f back)
/  C-f  C-p     find a file by name
:  C-t          run any command by name, or :42 to go to line 42
e               open $EDITOR at the cursor's line
V  v  S-↓       select lines, or drag with the mouse
yy  C-S-c       copy the code (y or C-c while selecting)
yp  yl          copy the path, or path:line (path:10-20 for a selection)
x               mark viewed and go to the next file
s               side by side or unified
w               wrap long lines
f  C-b          show or hide the file list
zR  zM          open or fold everything
r  F5           reload from git
q  ZZ           quit
```

Counts work the way they do in vim, so `5j` moves five lines and `42G` goes
to line 42. A key that starts a longer one, like `]` or `g`, waits for the
next key, and the status line lists what can follow. `Esc` cancels it.

A selection made with `V` or `v` keeps growing with every move until `Esc`.
One made with Shift or the mouse ends on the next move without Shift.
After `e` the diff reloads, so you see your edit straight away.

## Settings

Sift reads its settings from `~/.config/sift/config.toml`, or from
`$XDG_CONFIG_HOME/sift` if that's set. It's the same path on macOS. Settings go
under `[diff]`, the same table Glean reads them from, so a `[diff]` block copies
between the two files as it is. Every key is optional, so the file only needs
the ones you change:

```toml
[diff]
layout = "unified"         # or "side-by-side"
wrap = true
tab-width = 2
indicators = "bars"        # "signs" for git's + and -, or "none"
backgrounds = false        # changed lines keep only their word highlights
line-numbers = false
whitespace = "ignore"      # or "compare"
context = 5                # unchanged lines kept around a change
emphasis = "characters"    # "words" by default, or "each-word" or "none"
theme = "tokyo-night"      # or "terminal"
syntax-theme = "catppuccin_mocha"
```

Every key is also a flag with the same name, and a flag wins over the file for
that run. So `sift --layout side-by-side` undoes `layout = "unified"`, and
`--wrap=false` undoes `wrap = true`. A key Sift doesn't know is an error, with
the line it's on.

`:settings` (or `C-,`, where the terminal sends it) opens a page with every
setting and every key. A change there is written to `config.toml` straight away,
keeping your comments, and the diff takes it at once. A setting a flag set for
this run is marked, since the flag still wins over the file.

A `[keys]` table gives a command new keys in place of the ones it came with:

```toml
[keys]
toggle-viewed = ["v"]
next-hunk = ["]c", "F7"]
copy-path = []            # no key, but it's still in the palette
```

Keys are written the way the settings page shows them. `C-d` and `ctrl-d` are the same,
and so are `S-Tab` and `shift-Tab`. `]c` is `]` then `c`, and `"C-x C-s"` is
two keys in a row. These are the names:

```
palette  settings  reload  quit  back
next-file  previous-file  find-file  toggle-file-list  edit  copy
copy-selection  copy-path  copy-location  toggle-viewed  next-unviewed
previous-unviewed  close-file
next-hunk  previous-hunk  next-line  previous-line  scroll-line-down
scroll-line-up  half-page-down  half-page-up  page-down  page-up  top  bottom
go-to-line  cursor-line-to-middle  cursor-line-to-top  cursor-line-to-bottom
old-side  new-side  scroll-left  scroll-right  select  select-down  select-up
clear-selection  expand-fold  close-fold  toggle-fold  open-everything  fold-everything
toggle-layout  toggle-wrapping  toggle-whitespace
```

## Layout

    src/command.rs   the diff's commands and the keys they come with
    src/frame.rs     Sift's own keys, palette rows and status line
    src/diff         diff model, side-by-side alignment, wrapping
    src/git.rs       what changed between a revision and the worktree, index or another revision
    src/ui           the file list and the diff

The library is the diff. `command::keymap()` holds its keys, and
`frame::keymap()` joins Sift's own (quitting, the palette, finding a file) onto
them. The palette, the status line hints and the settings page's Keys tab are
all drawn from that one keymap, so they can't drift apart. A program that embeds
the diff joins `command::keymap()` into its own the same way, and a key both
use under the same condition is refused.

## Requires

Rust stable (edition 2024) and git ≥ 2.38.
