#!/bin/sh
# PreToolUse(Edit|Write): a dependency is a decision, not an edit.
# AGENTS.md says to ask before adding a crate; this makes it a prompt with a
# diff rather than an honour-system rule.

set -eu

file=$(jq -r '.tool_input.file_path // empty')

case "$file" in
    Cargo.toml | */Cargo.toml)
        printf '%s' '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"Cargo.toml: AGENTS.md requires asking before adding a crate."}}'
        ;;
esac
