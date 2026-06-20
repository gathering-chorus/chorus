#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# 3369-deep-health-resilience.bats — Tests for #3369
# What Jeff sees: the 6am window runs clean — daily-signal-scan completes instead
# of dying 2s in, and the vikunja-auth alert only fires on real auth failures.
#
# Root cause chain this card pins: deep-health.sh `set -e` + an unguarded
# `grep` in the liveness check died on the first retired LaunchAgent label
# (bridge-subscriber-*, retired by #3352 always-inject) → deep-health exits 1
# with ZERO output → daily-signal-scan's health_result=$(deep-health|head -1)
# dies under its own set -e → signal scan never completes, every morning.

DEEP_HEALTH="${HOME}/CascadeProjects/chorus-werk/silas-3369/platform/scripts/deep-health.sh"
SIGNAL_SCAN="${HOME}/CascadeProjects/chorus-werk/silas-3369/platform/scripts/daily-signal-scan.sh"

@test "deep-health liveness pid lookup survives a label missing from launchctl" {
  # the line-87 class: grep no-match (exit 1) inside $() under set -e kills the
  # script. The in-file idiom for this is `|| true` (line 214 already does it).
  grep -E 'pid=\$\(launchctl list.*grep "\$liveness_match"' "$DEEP_HEALTH" | grep -q '|| true'
}

@test "deep-health liveness list carries no retired bridge-subscriber agents" {
  # #3352 retired com.chorus.bridge-subscriber-{silas,wren,kade} (always-inject
  # replaced them). A liveness expectation for a retired agent is a standing
  # false alarm — same class as com.gathering.messaging in the 6:03 ops scan.
  run grep -E 'LIVENESS_LOGS=.*bridge-subscriber' "$DEEP_HEALTH"
  [ "$status" -ne 0 ]
}

@test "deep-health vikunja-auth detector does not substring-match 401 in card text" {
  # `grep -qE "401|403|..."` over cards CLI output fires on any card whose text
  # contains 401/403 (titles, latencies, ids). Detector must anchor on the
  # cards CLI's actual auth-error shape, not bare numbers.
  run grep -E 'grep -qE "401\|403' "$DEEP_HEALTH"
  [ "$status" -ne 0 ]
}

@test "deep-health syntax is valid bash" {
  bash -n "$DEEP_HEALTH"
}

@test "signal-scan tolerates deep-health failure instead of dying" {
  # health_result=$(deep-health|head -1) under set -euo pipefail: a nonzero
  # deep-health must not abort the scan — a red health check is signal CONTENT,
  # not a reason to produce no signal at all.
  grep -A2 'health_result=' "$SIGNAL_SCAN" | grep -qE '\|\| true|\|\| echo'
}

@test "signal-scan syntax is valid bash" {
  bash -n "$SIGNAL_SCAN"
}

@test "deep-health end-to-end: produces a summary even when checks fail" {
  # The silent-death regression: exit 1 with empty output. Failures are fine;
  # silence is not. (Read-only probes; same class the nightly already runs.)
  run bash "$DEEP_HEALTH"
  [ -n "$output" ]
  echo "$output" | grep -qiE "summary|failures|warnings|healthy|ok"
}
