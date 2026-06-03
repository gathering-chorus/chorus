#!/usr/bin/env bash
# werk-mcp_test.sh — hermetic test for werk-mcp.sh's 8-step flow (#3200).
# Shims curl (the MCP endpoint) + gh on PATH; asserts the step sequence, the
# demo→merge→HARD-STOP shape, and fail-loud. No network, no real MCP, no main touched.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/werk-mcp.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export CALLS_LOG="$TMP/calls"; : > "$CALLS_LOG"

# curl shim: initialize → emit Mcp-Session-Id header; tools/call → emit SSE data + log the tool.
cat > "$TMP/curl" <<'SHIM'
#!/usr/bin/env bash
prev=""; payload=""
for a in "$@"; do [ "$prev" = "-d" ] && payload="$a"; prev="$a"; done
case "$payload" in
  *initialize*)                echo "Mcp-Session-Id: test-sid" ;;
  *notifications/initialized*) : ;;
  *tools/call*)
     tool=$(printf '%s' "$payload" | sed -n 's/.*"name":"\([^"]*\)".*/\1/p')
     echo "tools/call $tool" >> "$CALLS_LOG"
     printf 'data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}\n' ;;
esac
exit 0
SHIM
chmod +x "$TMP/curl"

# gh shim: present for any incidental gh use + succeed. (Step 5 merge now goes through
# the MCP werk-merge verb via the curl shim, #3175 — not inline gh.)
cat > "$TMP/gh" <<'SHIM'
#!/usr/bin/env bash
echo "gh $*" >> "$CALLS_LOG"
exit 0
SHIM
chmod +x "$TMP/gh"

export PATH="$TMP:$PATH"
export CHORUS_WERK_BASE="$TMP/werk-base"
mkdir -p "$TMP/werk-base/kade-9999"   # fake werk dir for step-5 gh cd

out="$("$SCRIPT" kade 9999 jeff 2>&1)"; rc=$?
fail() { echo "FAIL: $1"; echo "--- output ---"; echo "$out"; echo "--- calls ---"; cat "$CALLS_LOG"; exit 1; }

# AC#1 — rename: new present, old gone.
[ -f "$HERE/werk-mcp.sh" ]      || fail "werk-mcp.sh missing"
[ -f "$HERE/werk-acp-mcp.sh" ]  && fail "old werk-acp-mcp.sh still present"

# AC#2/#3/#4 — the demo half runs for real, in order, then step 5 merge via werk-merge.
grep -q "1 commit"      <<<"$out" || fail "step 1 commit missing"
grep -q "2 push"        <<<"$out" || fail "step 2 push missing"
grep -q "3 build-demo"  <<<"$out" || fail "step 3 build-demo missing"
grep -q "4 deploy-demo" <<<"$out" || fail "step 4 deploy-demo missing"
grep -q "4 env-up"      <<<"$out" || fail "step 4 env-up (running instance) missing"
grep -q "5 merge"       <<<"$out" || fail "step 5 werk-merge missing"
# #3175: step 5 is the real werk-merge MCP verb now — NOT the interim gh path.
grep -q "interim"       <<<"$out" && fail "step 5 still labeled interim — werk-merge (#3175) should have retired it"

# AC#5 — hard-stop at the deploy-from-main gap; prod steps + accept do NOT run.
grep -q "\[BLOCKED\] deploy-from-main" <<<"$out" || fail "step 6 hard-stop (BLOCKED deploy-from-main) missing"
grep -q "\[done\]" <<<"$out" && fail "reached [done] — prod steps ran; must hard-stop"
[ "$rc" -eq 3 ] || fail "expected hard-stop exit 3, got $rc"

# The demo-half MCP call sequence, in order, by tool name.
seq="$(grep '^tools/call' "$CALLS_LOG" | awk '{print $2}' | paste -sd, -)"
# #3175: step 5 merge is now a real MCP verb (werk-merge) in the sequence, not inline gh.
expected="werk-commit,werk-push,chorus_build,chorus_deploy,chorus_env_up,werk-merge"
[ "$seq" = "$expected" ] || fail "MCP call sequence: got [$seq] expected [$expected]"

# AC#5/#6 — no canonical deploy and no accept ran (gated behind the blocked prod deploy).
grep -q "werk-accept"      "$CALLS_LOG" && fail "werk-accept ran — accept must stay gated behind a real prod deploy"
grep -q 'target.*canonical' "$CALLS_LOG" && fail "canonical deploy ran — must hard-stop before deploying werk-content to prod"

echo "PASS: werk-mcp 8-step flow — demo real, merge via werk-merge verb (#3175), hard-stop at deploy-from-main, accept gated"
