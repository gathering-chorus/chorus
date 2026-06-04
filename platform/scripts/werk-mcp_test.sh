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
     tgt=$(printf '%s' "$payload" | sed -n 's/.*"target":"\([^"]*\)".*/\1/p')
     echo "tools/call $tool${tgt:+ target=$tgt}" >> "$CALLS_LOG"
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

# werk-demo shim (#3116): demo is a binary step (4.5) in the flat sequence, not an
# MCP tool — shim it to log + succeed so the hermetic flow reaches merge.
cat > "$TMP/werk-demo" <<'SHIM'
#!/usr/bin/env bash
echo "werk-demo $*" >> "$CALLS_LOG"
exit 0
SHIM
chmod +x "$TMP/werk-demo"

export PATH="$TMP:$PATH"
export CHORUS_WERK_BASE="$TMP/werk-base"
mkdir -p "$TMP/werk-base/kade-9999"   # fake werk dir for step-5 gh cd

# #3234 — CHORUS_HOME = a temp git repo 1 commit BEHIND its origin/main, so step 5.5's
# ff-sync (scripts-land-live) runs hermetically and we can assert it advanced canonical.
ORIGIN="$TMP/origin"; CANON="$TMP/canon"
git init -q -b main "$ORIGIN"
git -C "$ORIGIN" -c user.email=t -c user.name=t commit -q --allow-empty -m c1
git -C "$ORIGIN" -c user.email=t -c user.name=t commit -q --allow-empty -m c2
git clone -q "$ORIGIN" "$CANON" 2>/dev/null
git -C "$CANON" reset -q --hard HEAD~1   # canonical now 1 behind origin/main
export CHORUS_HOME="$CANON"

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
grep -q "4.5 demo"      <<<"$out" || fail "step 4.5 demo (werk-demo proving ceremony, #3116) missing"
grep -q "5 merge"       <<<"$out" || fail "step 5 werk-merge missing"
# #3175: step 5 is the real werk-merge MCP verb now — NOT the interim gh path.
grep -q "interim"       <<<"$out" && fail "step 5 still labeled interim — werk-merge (#3175) should have retired it"

# #3222 retired the BLOCKED hard-stop; #3234: prod deploy runs, accept does NOT (human's hand).
grep -q "\[BLOCKED\] deploy-from-main" <<<"$out" && fail "stale BLOCKED hard-stop still present — #3222 retired it"

# #3234 AC2 — step 5.5 ff-syncs canonical so SCRIPT changes land live, and it actually advanced.
grep -q "5.5 sync-canonical" <<<"$out" || fail "step 5.5 sync-canonical missing"
[ "$(git -C "$CANON" rev-parse HEAD)" = "$(git -C "$ORIGIN" rev-parse main)" ] \
  || fail "5.5 did NOT ff canonical to origin/main (scripts would stay stale)"

# #3222 — step 6 deploy-prod runs (canonical deploy from main is no longer blocked).
grep -q "6 deploy-prod" <<<"$out" || fail "step 6 deploy-prod missing"

# #3234 AC1 — ACCEPT IS THE HUMAN'S HAND: werk-mcp STOPS before accept, prints the
# accept instruction, and does NOT auto-fire werk-accept.
grep -q "NOT yet accepted"      <<<"$out" || fail "missing 'NOT yet accepted' — must stop before accept"
grep -q "werk-accept 9999" <<<"$out" || fail "missing the 'run werk-accept yourself' instruction for the human"
grep -q "7 accept" <<<"$out" && fail "step 7 accept still present — accept must be the human's separate act (#3234)"
[ "$rc" -eq 0 ] || fail "expected clean exit 0 (no hard-stop), got $rc"

# MCP call sequence: demo half + step-5 merge + step-6 canonical deploy. NO werk-accept.
seq="$(grep '^tools/call' "$CALLS_LOG" | awk '{print $2}' | paste -sd, -)"
expected="werk-commit,werk-push,chorus_build,chorus_deploy,chorus_env_up,werk-merge,chorus_deploy"
[ "$seq" = "$expected" ] || fail "MCP call sequence: got [$seq] expected [$expected]"
grep -q "werk-accept" "$CALLS_LOG" && fail "werk-accept ran via the script — accept must be the human's hand (DEC-048, #3234)"
grep -q 'target.*canonical' "$CALLS_LOG" || fail "step 6 canonical deploy did NOT run (#3222 should deploy from main)"

# #3234 (Kade catch) — when 5.5 CAN'T ff (canonical DIVERGED), the "live" claim must
# DOWNGRADE to "NOT yet live" — never claim live while scripts are stale (the merged≠live
# lie in miniature). Diverge: origin/main AND canonical each get a different commit, so
# ff-only genuinely fails (not just "already up to date").
git -C "$ORIGIN" -c user.email=t -c user.name=t commit -q --allow-empty -m "origin advances"
git -C "$CANON"  -c user.email=t -c user.name=t commit -q --allow-empty -m "local divergence"
out2="$("$SCRIPT" kade 9999 jeff 2>&1)"
grep -q "NOT yet live"     <<<"$out2" || fail "5.5 failure did not downgrade the claim to 'NOT yet live' (Kade #3234)"
grep -q "deployed + LIVE"  <<<"$out2" && fail "claimed LIVE despite 5.5 failure — the merged≠live lie this card kills"

echo "PASS: werk-mcp flow — demo real, merge (#3175), 5.5 canonical-sync (scripts live), step-6 deploy-from-main (#3222), accept is the human's hand + live-claim-conditional-on-5.5 (#3234)"
