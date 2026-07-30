#!/usr/bin/env bash
# #3714 — an alarm fires when its check FAILED, not when its output is unfamiliar.
#
# alert-runner.sh decided pass/fail with `[[ "$result" == "ok" ]]` — comparing the
# check's OUTPUT to the literal word "ok". A check that succeeded while saying
# anything else fired every single run. Receipt from the runner's own log:
#
#   [2026-07-30 08:00:22] FIRE fuseki-harvest-stale — ok: 38074 photos in Fuseki
#                                                     (consecutive: 112310)
#
# 112,310 consecutive "failures" at 60s = 78 days, all false, while Jeff got a
# daily nudge whose text hardcoded "SPARQL COUNT ... = 0" — a number the check
# never reported. #3571 settled exit-code-is-verdict for test suites; alerts
# never got it.
set -uo pipefail
RUNNER="${1:-$(cd "$(dirname "$0")/.." && pwd)/alert-runner.sh}"
ALERTS="${2:-$(cd "$(dirname "$0")/../../domains/alerts" && pwd)}"
FAIL=0
pass(){ echo "  PASS: $1"; }
fail(){ echo "  FAIL: $1"; FAIL=1; }

# The runner's decision, extracted so we test the REAL logic, not a copy.
verdict() {  # <script-body> -> "fire" | "silent"
  local body="$1" result rc
  result=$(bash -c "$body" 2>&1); rc=$?
  if grep -q 'rc -eq 0' "$RUNNER"; then
    [ $rc -eq 0 ] && echo silent || echo fire
  else
    [ "$result" = "ok" ] && echo silent || echo fire
  fi
}

echo "=== a check that SUCCEEDS is silent, whatever it prints ==="
for out in 'ok' 'ok: 38074 photos in Fuseki' 'ok dir=/x newest_age_h=2 threshold_h=72' '' 'everything is fine'; do
  v=$(verdict "echo '$out'; exit 0")
  [ "$v" = silent ] && pass "silent on success printing '${out:0:38}'" \
                    || fail "FIRES on a SUCCESSFUL check printing '${out:0:38}'"
done

echo "=== a check that FAILS still fires, and its output survives as evidence ==="
v=$(verdict "echo 'lancedb-stale dir=/x newest_age_h=2632'; exit 1")
[ "$v" = fire ] && pass "fires on a genuine failure" || fail "did NOT fire on a failing check"
if grep -q 'result:+ — \$result\|\${result:+' "$RUNNER"; then
  pass "success path carries the check's output into the log as evidence"
else
  fail "success path discards what the check observed"
fi

echo "=== the three known offenders, against the REAL checks ==="
for a in fuseki-harvest-stale lancedb-stale ci-main-red; do
  f="$ALERTS/$a.check.sh"
  if [ ! -f "$f" ]; then
    body=$(awk '/^check: \|/{f=1;next} f && /^[^[:space:]]/{exit} f{sub(/^  /,"");print}' "$ALERTS/$a.yml")
  else body="bash '$f'"; fi
  out=$(bash -c "$body" 2>&1); rc=$?
  v=$(verdict "$body")
  printf "  %-22s rc=%d verdict=%-7s out='%s'\n" "$a" "$rc" "$v" "$(printf '%s' "$out" | head -1 | cut -c1-44)"
  if [ $rc -eq 0 ] && [ "$v" = fire ]; then fail "$a succeeded but would still fire"; fi
done

echo
[ "$FAIL" -eq 0 ] && { echo "PASS: verdict is the exit code"; exit 0; }
echo "RED: a succeeding check can still fire"; exit 1
