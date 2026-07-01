#!/bin/bash
# #3537 + #3571 — Contract test for daily-review-quality.sh's suite classifier.
#
# #3537 guarded a lint EMITTER/CONSUMER kind-switch gap. #3571 fixes the deeper
# bug: the SUITE line is `SUITE|kind|path|owner|STATUS|summary` where STATUS is
# nightly-suites.sh's rc-derived verdict (pass|fail|skip from $rc), but the
# consumer IGNORED it and re-parsed `summary` — so a passing cargo/npm/coverage
# crate whose output the parser couldn't read was reported "DID NOT RUN" or
# "1/1 failed". ~37 false reds/night. Fix: EXIT-CODE IS THE VERDICT, parse is
# ENRICHMENT (counts only). classify_suite now takes STATUS.
#
# Sources daily-review-quality.sh and calls the REAL classify_suite (the script
# self-returns when sourced, before its 6am main flow) — a regression in the
# actual script fails this test. NOT a mirror of the logic.
set -u
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RQ="$SCRIPT_DIR/daily-review-quality.sh"
[ -f "$RQ" ] || { echo "FAIL: cannot find $RQ"; exit 1; }
# shellcheck disable=SC1090
source "$RQ"
command -v classify_suite >/dev/null || { echo "FAIL: daily-review-quality.sh did not expose classify_suite (source-guard or extraction broke)"; exit 1; }

PASS=0; FAIL=0

# classify_suite <kind> <status> <summary> → "verdict|passed|failed|total"
# verdict ∈ green|fail|broke|skip|norun. bucket() maps to the reader's report buckets.
bucket() {
  local v p f t
  IFS='|' read -r v p f t <<< "$(classify_suite "$1" "$2" "$3")"
  case "$v" in
    green) echo "GREEN:${p}" ;;
    fail)  echo "FAILED:${f}/${t}" ;;
    broke) echo "BROKE" ;;
    skip)  echo "SKIP" ;;
    *)     echo "DID-NOT-RUN" ;;
  esac
}
check() {
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "ok   - $1 ($2)"
  else FAIL=$((FAIL+1)); echo "FAIL - $1: got '$2' want '$3'"; fi
}

# ── THE #3571 FIX: status=pass is GREEN even when the summary is unparseable ──
# (the exact false-red: a suite that RAN CLEAN but whose output the parser can't
#  read was reported DID-NOT-RUN / 1-of-1-failed. These are the ~37/night.)
check "cargo pass + unparseable summary → GREEN (was false DID-NOT-RUN)" \
  "$(bucket cargo pass 'Compiling werk-push v0.1.0 / Finished test profile')" "GREEN:0"
check "npm pass + no jest-summary line → GREEN (was false DID-NOT-RUN)" \
  "$(bucket npm pass 'ran, produced no parseable Tests: line')" "GREEN:0"
check "coverage pass (percent output, no test count) → GREEN (a nightly DID-NOT-RUN)" \
  "$(bucket coverage pass 'statements 82.10% ( 1234/1503 )')" "GREEN:0"
check "smoke pass → GREEN" \
  "$(bucket smoke pass 'smoke-check: all endpoints 200')" "GREEN:0"

# ── status=fail is RED even when unparseable — never hidden as DID-NOT-RUN ──
check "cargo compile-fail (rc≠0, no test-result line) → BROKE, distinct from test-fail" \
  "$(bucket cargo fail 'error[E0432]: unresolved import / error: could not compile foo')" "BROKE"
check "npm real test-fail → FAILED with its count (parse enriches)" \
  "$(bucket npm fail 'Tests: 40 total, 3 failed, 37 passed')" "FAILED:3/40"
check "cargo real test-fail → FAILED with its count" \
  "$(bucket cargo fail 'suites: 12 ok, 2 failed')" "FAILED:2/14"

# ── status=skip is SKIP (stack-gated live suites, #3557), not fail/no-run ──
check "live-stack suite skipped (no stack) → SKIP" \
  "$(bucket smoke skip 'skipped — no live stack (#3557)')" "SKIP"

# ── genuine no-run: empty status = the suite produced no verdict at all ──
check "empty status → DID-NOT-RUN (real signal preserved, e.g. crashed pre-verdict)" \
  "$(bucket cargo '' '')" "DID-NOT-RUN"

# ── no regressions on the shapes #3537 handled (now status-aware) ──
check "lint clean (pass) → GREEN (the #3537 false-red)" \
  "$(bucket lint pass '1 pass, 0 fail (lint:ratchet clean — 0 over baseline)')" "GREEN:1"
check "lint drifted (fail) → FAILED" \
  "$(bucket lint fail '0 pass, 1 fail (lint:ratchet drifted rc=1 — 3 over baseline)')" "FAILED:1/1"
check "shell pass parses counts" \
  "$(bucket shell pass '5 pass, 0 fail')" "GREEN:5"
check "cargo pass parses counts" \
  "$(bucket cargo pass 'suites: 12 ok, 0 failed')" "GREEN:12"
check "npm pass parses counts" \
  "$(bucket npm pass 'Tests: 40 total, 0 failed, 40 passed')" "GREEN:40"

echo "---"; echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
