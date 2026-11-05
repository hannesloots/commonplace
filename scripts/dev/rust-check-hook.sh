#!/bin/sh
# Optional Claude Code PostToolUse hook for working on the Rust code.
# After an edit to a .rs file, format the workspace and feed the first clippy
# diagnostics back so the agent fixes them immediately. Enable it in
# .claude/settings.json (see AGENTS.md).
input="$(cat)"
case "$input" in
  *'.rs"'*) ;;
  *) exit 0 ;;
esac
cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)" || exit 0
cargo fmt --all 2>/dev/null
out="$(cargo clippy --workspace --all-targets --message-format=short 2>&1 | grep -E '^(crates|error)' | head -30)"
[ -z "$out" ] && exit 0
printf 'cargo clippy reports:\n%s\n' "$out" >&2
exit 2
