#!/bin/sh
# Stop hook: if the working directory is inside a commonplace and markdown changed,
# run the deterministic checks and feed failures back so the agent fixes them
# before finishing. Exits 0 (no-op) anywhere else, so the hook is harmless in
# other projects.
dir="$PWD"
root=""
while [ "$dir" != "/" ] && [ -n "$dir" ]; do
  if [ -f "$dir/commonplace.toml" ]; then root="$dir"; break; fi
  dir="$(dirname "$dir")"
done
[ -z "$root" ] && exit 0
launcher="${CLAUDE_PLUGIN_ROOT}/skills/commonplace/scripts/commonplace"
[ -x "$launcher" ] || launcher="${CLAUDE_PLUGIN_ROOT}/bin/commonplace"
[ -f "$launcher" ] || exit 0
# Guard against loops: the hook input carries stop_hook_active when we already blocked once.
input="$(cat 2>/dev/null)"
case "$input" in *'"stop_hook_active":true'*|*'"stop_hook_active": true'*) exit 0 ;; esac
out="$(cd "$root" && sh "$launcher" lint --changed --no-cite 2>&1; echo "exit=$?")"
cite="$(cd "$root" && sh "$launcher" cite verify --changed 2>&1; echo "exit=$?")"
if echo "$out" | grep -q 'exit=1' || echo "$cite" | grep -q 'exit=1'; then
  printf 'commonplace checks found problems in %s. Fix the errors below (or explain to the user why they stay), then finish.\n\n%s\n\n%s\n' "$root" "$out" "$cite" >&2
  exit 2
fi
exit 0
