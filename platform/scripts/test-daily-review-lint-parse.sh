#!/bin/bash
# #3537 — Contract guard for daily-review-quality.sh's suite-kind classifier.
#
# Regression guarded: #3484 added a lint EMITTER in nightly-suites.sh
# (run_lint_ratchet → "SUITE|lint|<path>|<owner>|<status>|N pass, N fail") but the
# CONSUMER's kind-switch in daily-review-quality.sh had no `lint)` case, so every
# lint line fell through → parsed=no → a false "DID NOT RUN (no parseable test
# output)" nudge every nightly though lint was actually green. Fix: `shell|lint)`.
#
# This asserts the consumer's lint/shell branch classifies the real emitted shapes.
# It mirrors the `shell|lint)` branch from daily-review-quality.sh; if that branch
# loses `lint` (or the regex drifts), the lint-clean case below flips to DID-NOT-RUN
# and this test fails. Kept in lockstep with daily-review-quality.sh's case block.
set -u
PASS=0; FAIL=0

# Mirror of daily-review-quality.sh `shell|lint)` classification.
classify() {
  local summary="$1"
  local s_total=0 s_failed=0 s_passed=0 parsed="no"
  if echo "$summary" | grep -qE "[0-9]+ (pass|ok)"; then
    s_passed=$(echo "$summary" | grep -oE '[0-9]+ (pass|ok)' | head -1 | grep -oE '[0-9]+' || echo 0)
    s_failed=$(echo "$summary" | grep -oE '[0-9]+ fail' | head -1 | grep -oE '[0-9]+' || echo 0)
    s_total=$((s_passed + s_failed)); [ "$s_total" -gt 0 ] && parsed="yes"
  fi
  # bucket: DID-NOT-RUN | FAILED | GREEN — the three daily-review outcomes
  if   [ "$parsed" = "no" ];     then echo "DID-NOT-RUN"
  elif [ "$s_failed" -gt 0 ];    then echo "FAILED:${s_failed}/${s_total}"
  else echo "GREEN:${s_passed}"; fi
}

check() {
  local label="$1" got="$2" want="$3"
  if [ "$got" = "$want" ]; then PASS=$((PASS+1)); echo "ok   - $label ($got)"
  else FAIL=$((FAIL+1)); echo "FAIL - $label: got '$got' want '$want'"; fi
}

# The exact shapes run_lint_ratchet emits (#3484).
check "lint clean → GREEN (the false-red that bit every nightly)" \
  "$(classify '1 pass, 0 fail (lint:ratchet clean — 0 over baseline)')" "GREEN:1"
check "lint drifted → FAILED, not DID-NOT-RUN" \
  "$(classify '0 pass, 1 fail (lint:ratchet drifted rc=1 — 3 over baseline)')" "FAILED:1/1"
check "shell suite still classifies (no regression)" \
  "$(classify '5 pass, 0 fail')" "GREEN:5"

# Structural lockstep: the real consumer must still group lint WITH shell.
RQ="$(cd "$(dirname "$0")" && pwd)/daily-review-quality.sh"
if grep -qE '^\s*shell\|lint\)' "$RQ"; then PASS=$((PASS+1)); echo "ok   - daily-review-quality.sh groups lint with shell"
else FAIL=$((FAIL+1)); echo "FAIL - daily-review-quality.sh lost the shell|lint) case → lint will false-red"; fi

echo "---"; echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
