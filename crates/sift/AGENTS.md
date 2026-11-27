# AGENTS.md

Sift — a terminal diff viewer over git. `src/diff` is the diff model,
`src/git.rs` the git plumbing, `src/command.rs` the diff's commands and their keys,
`src/frame.rs` the decisions of Sift's own frame (its keys, palette rows and
status line), `src/ui` the file list and the diff.

Glean hosts the diff through `sift::ui::Review`, its annotations, and
`sift::command::keymap()`. Those, and `sift::theme`, are the library surface Glean
relies on. `src/main.rs` is thin glue: anything worth a test lives in a library
function or keymap and is tested there.

The workspace rules — style, commands, the gate, commits — are in the root
@../../AGENTS.md.
