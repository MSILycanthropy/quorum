# AGENTS.md

Glean — a terminal client for GitHub pull requests. It lists the pull requests
of the repository you're in, and opens one in Sift, the diff viewer it is built
on, to review, comment, approve and merge. Its screens recreate GitHub's own
pages in the terminal.

`src/model` is what Glean shows, in its own terms: the repository on the
forge, pull requests, threads, comments, reviews, checks.

Screens use the model and nothing else. A forge's API types never leave the
module that talks to that forge: it converts them into the model. GitHub is the
only forge for now; Bitbucket may follow, and this is what keeps that a second
converter rather than a rewrite. There is no forge trait until a second forge
exists.

Sift is a library dependency (`crates/sift`). Glean talks to Sift through
`sift::ui::Surface`, its annotations and its host bindings, and colours with
`sift::theme`. Any crate both use, `ratatui` above all, comes from the
workspace's dependencies, so their types line up.

`src/github` is the only module that knows about HTTP, GraphQL or GitHub's
JSON. GitHub is reached over its GraphQL API directly with `ureq`, not by
running `gh`; the one thing taken from `gh` is the token, from `gh auth token`.
Calls block, so they run off the thread that draws and hand their results to
the loop over a channel, the way Sift's watcher does. Each query is a
`.graphql` file beside the module that reads its answer.

## Tests

Tests never reach GitHub. Anything that talks to the network takes its
responses from fixtures.

The workspace rules — style, commands, the gate, commits — are in the root
@../../AGENTS.md.
