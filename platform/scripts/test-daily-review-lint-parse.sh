#!/bin/bash
# #3537 — Contract test for daily-review-quality.sh's suite-kind classifier.
#
# Regression guarded: #3484 added a lint EMITTER in nightly-suites.sh
# (run_lint_ratchet → "SUITE|lint|<path>|<owner>|<status>|N pass, N fail") but the
# CONSUMER's kind-switch in daily-review-quality.sh had no `lint)` case, so every lint
# line fell through → parsed=no → a false "DID NOT RUN (no parseable test output)" nudge
# every nightly though lint was green. Fix: `shell|lint)`.
#
# This SOURCES daily-review-quality.sh and calls its REAL classify_suite function (the
# script self-returns when sourced, before its 6am main flow) — so a regression in the
# actual script fails this test. NOT a mirror of the logic (Wren's #3537 review).
set -u
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RQ="$SCRIPT_DIR/daily-review-quality.sh"
[ -f "$RQ" ] || { echo "FAIL: cannot find $RQ"; exit 1; }
# shellcheck disable=SC1090
source "$RQ"
command -v classify_suite >/dev/null || { echo "FAIL: daily-review-quality.sh did not expose classify_suite (source-guard or extraction broke)"; exit 1; }

PASS=0; FAIL=0

# Route classify_suite's "parsed|pass|fail|total" through the SAME 3 outcomes the
# consumer buckets into: DID-NOT-RUN | FAILED:f/t | GREEN:p.
bucket() {
  local parsed p f t
  IFS='|' read -r parsed p f t <<< "$(classify_suite "$1" "$2")"
  if   [ "$parsed" = "no" ]; then echo "DID-NOT-RUN"
  elif [ "$f" -gt 0 ];       then echo "FAILED:${f}/${t}"
  else echo "GREEN:${p}"; fi
}
check() {
  if [ "$2" = "$3" ]; then PASS=$((PASS+1)); echo "ok   - $1 ($2)"
  else FAIL=$((FAIL+1)); echo "FAIL - $1: got '$2' want '$3'"; fi
}

check "lint clean → GREEN (the false-red that bit every nightly)" \
  "$(bucket lint '1 pass, 0 fail (lint:ratchet clean — 0 over baseline)')" "GREEN:1"
check "lint drifted → FAILED, not DID-NOT-RUN" \
  "$(bucket lint '0 pass, 1 fail (lint:ratchet drifted rc=1 — 3 over baseline)')" "FAILED:1/1"
check "shell suite still classifies (no regression)" \
  "$(bucket shell '5 pass, 0 fail')" "GREEN:5"
check "cargo suite still classifies (no regression)" \
  "$(bucket cargo 'suites: 12 ok, 0 failed')" "GREEN:12"
check "npm suite still classifies (no regression)" \
  "$(bucket npm 'Tests: 40 total, 0 failed, 40 passed')" "GREEN:40"
check "unparseable lint → DID-NOT-RUN (real signal preserved)" \
  "$(bucket lint 'garbage with no counts')" "DID-NOT-RUN"

echo "---"; echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
