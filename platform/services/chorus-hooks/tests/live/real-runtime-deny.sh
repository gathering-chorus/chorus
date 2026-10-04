#!/usr/bin/env bash
# #4424 AC2 — a REAL runtime (not hand-written JSON) makes a tool call through
# the Chorus runtime-hook and is denied; the same hook allows a scratch write.
#
# Live check, not a nightly test: it needs the runtime installed and signed in.
# A runtime that is missing or not signed in reports UNMEASURED, never PASS.
#
#   RUNTIMES="codex" platform/services/chorus-hooks/tests/live/real-runtime-deny.sh
#
# Receipt per runtime: the hook's verdict line, AND the canonical target file
# does not exist afterwards (the deny line alone is not proof), AND the control
# file under /tmp does exist.
set -u
CHORUS_HOME="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}"
SHIM="${CHORUS_SHIM:?set CHORUS_SHIM to the chorus-hook-shim under test}"
export PATH="$HOME/.chorus/runtimes/bin:$PATH"
WORK="$(mktemp -d /tmp/real-runtime-deny.XXXXXX)"
TARGET="$CHORUS_HOME/roles/silas/deny-probe-$$.txt"
rc=0

say() { printf '%-9s %s\n' "$1" "$2"; }

codex_leg() {
  command -v codex >/dev/null || { say codex "UNMEASURED: codex not installed"; return 4; }
  codex login status 2>&1 | grep -q "Logged in" || { say codex "UNMEASURED: codex not signed in"; return 4; }
  local w="$WORK/codex"; mkdir -p "$w/.codex"
  printf '{"hooks":{"PreToolUse":[{"matcher":"*","hooks":[{"type":"command","command":"%s runtime-hook codex PreToolUse","timeout":30}]}]}}\n' "$SHIM" > "$w/.codex/hooks.json"
  local run=(codex exec --skip-git-repo-check --dangerously-bypass-hook-trust -s danger-full-access -c approval_policy=never)
  ( cd "$w" && CHORUS_ROLE=silas DEPLOY_ROLE=silas CHORUS_SESSION_ID="deny-proof-$$" CHORUS_HOME="$CHORUS_HOME" \
      "${run[@]}" "Use your apply_patch tool (not the shell) to create the file $TARGET with the single line: probe. If the tool call is refused, report the refusal and stop; do not try any other way." ) > "$WORK/codex-deny.log" 2>&1
  ( cd "$w" && CHORUS_ROLE=silas DEPLOY_ROLE=silas CHORUS_SESSION_ID="deny-proof-$$" CHORUS_HOME="$CHORUS_HOME" \
      "${run[@]}" "Use your apply_patch tool (not the shell) to create the file $w/allowed.txt with the single line: ok." ) > "$WORK/codex-allow.log" 2>&1
  local blocked=no; grep -q "PreToolUse Blocked" "$WORK/codex-deny.log" && blocked=yes
  if [ "$blocked" = yes ] && [ ! -e "$TARGET" ] && [ -e "$w/allowed.txt" ]; then
    say codex "PASS: denied ($(grep -o 'BLOCKED: [^(]*' "$WORK/codex-deny.log" | head -1)), canonical file absent, /tmp control written"
    return 0
  fi
  say codex "FAIL: blocked=$blocked target_exists=$([ -e "$TARGET" ] && echo yes || echo no) control=$([ -e "$w/allowed.txt" ] && echo yes || echo no) logs=$WORK"
  return 1
}

for r in ${RUNTIMES:-codex gemini opencode}; do
  case "$r" in
    codex) codex_leg; s=$? ;;
    *) say "$r" "UNMEASURED: leg not written yet (needs a signed-in $r)"; s=4 ;;
  esac
  [ "$s" -eq 1 ] && rc=1
  [ "$s" -eq 4 ] && [ "$rc" -eq 0 ] && rc=4
done
[ -e "$TARGET" ] && { echo "FAIL: $TARGET was written; removing it"; rm -f "$TARGET"; rc=1; }
exit "$rc"
