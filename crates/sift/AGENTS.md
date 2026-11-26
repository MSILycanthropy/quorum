# AGENTS.md

Sift — a terminal diff viewer over git. `src/diff` is the diff model,
`src/git.rs` the git plumbing, `src/command.rs` the key catalog every surface
reads from, `src/ui` the terminal surfaces.

Glean hosts Sift through `sift::ui::Surface`, its annotations and its host
bindings. Those, and `sift::theme`, are the library surface Glean relies on.

The workspace rules — style, commands, the gate, commits — are in the root
@../../AGENTS.md.
