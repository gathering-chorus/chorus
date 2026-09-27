#!/usr/bin/env bash
# @test-type: unit — runs session-live-check on fixture rows; no store, no network
#
# #4351 — the nightly read two non-failures as red: Wren idle at 4am, and a
# leftover zz-probe row picked as Kade's newest session. Idle and residue must
# read green; a login with no run, or a seen hook that never wrote, must read red.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$ROOT/platform/scripts/session-live-check"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
mk() { # $1 dir, $2 lastSeenAt, $3 with-run (1/0), $4 with-probe (1/0)
  mkdir -p "$1"
  probe=""; [ "$4" = 1 ] && probe=',{"name":"zz-probe-20260923t193502z-session","ownedBy":"principal-kade","sessionState":"zz probe","startedAt":"zz","actsAs":"zz"}'
  printf '{"data":[{"name":"kade-abc","ownedBy":"principal-kade","sessionState":"open","actsAs":"role-kade","startedAt":"2026-09-26T16:32:05Z","lastSeenAt":"%s"}%s]}' "$2" "$probe" > "$1/sessions.json"
  if [ "$3" = 1 ]; then printf '{"data":[{"name":"kade-run1","runOf":"session-kade-abc"}]}' > "$1/runs.json"; else echo '{"data":[]}' > "$1/runs.json"; fi
  printf '{"data":[{"name":"p1","presenceOf":"session-run-kade-run1"}]}' > "$1/presences.json"
  printf '{"data":[{"name":"c1","contextOf":"session-run-kade-run1","contextKind":"boot"}]}' > "$1/contexts.json"
}
run() { SESSION_LIVE_FIXTURE="$1" bash "$CHECK" kade >"$TMP/out" 2>&1; echo $?; }
ok() { [ "$2" = "$3" ] && { echo "PASS $1"; pass=$((pass+1)); } || { echo "FAIL $1 (exit $2, want $3)"; cat "$TMP/out"; fail=$((fail+1)); }; }
mk "$TMP/idle" "2026-09-26T21:00:00Z" 1 1;  ok "idle for hours with a probe row beside it reads green" "$(run "$TMP/idle")" 0
grep -q "kade-abc" "$TMP/out" && { echo "PASS the real login row was judged, not the probe"; pass=$((pass+1)); } || { echo "FAIL the probe row was judged"; fail=$((fail+1)); }
mk "$TMP/norun" "2026-09-26T21:00:00Z" 0 0; ok "negative proof: a login with no live run reads red" "$(run "$TMP/norun")" 1
mk "$TMP/unseen" "2026-09-26T10:00:00Z" 1 0; ok "negative proof: a seen hook that never wrote since start reads red" "$(run "$TMP/unseen")" 1
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
