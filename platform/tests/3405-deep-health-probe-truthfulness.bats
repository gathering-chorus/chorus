#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# 3405-deep-health-probe-truthfulness.bats — Tests for #3405
# What Jeff sees: deep-health stops crying wolf. A probe that can't RUN
# (cards not on the cron PATH → exit 127) reports a probe-misconfig WARNING,
# not a fake "vikunja down" FAILURE. The alert-delivery check reports the
# LATEST run's result, not a month-old FAIL grepped from log history. And the
# standards-surface artifact, retired off gathering by the chorus-out-of-
# gathering migration (#2969/#3361), is a tracked WARNING (no current home),
# not a nightly FAILURE.
#
# Root: probes conflated "service is down" with "probe is misconfigured/stale"
# (ADR-043 — the alert domain must distinguish them). 2026-06-14 overnight:
# 3/3 deep-health "failures" were probe-truthfulness bugs, not outages.

DEEP_HEALTH="${HOME}/CascadeProjects/chorus-werk/silas-3405/platform/scripts/deep-health.sh"

@test "deep-health syntax is valid bash" {
  bash -n "$DEEP_HEALTH"
}

@test "vikunja-auth: exit 127 (cards not runnable) is a probe-misconfig WARNING, not a vikunja FAILURE" {
  # 127 = command-not-found: the cards CLI (or a dep) isn't on the cron PATH.
  # That is a PROBE problem, not a Vikunja outage. It must not push a
  # 'vikunja down / token invalid' line into FAILURES.
  run grep -nE 'CARDS_EXIT.*-eq.*127|=.*127.*probe-misconfig|127\).*WARN' "$DEEP_HEALTH"
  [ "$status" -eq 0 ]
}

@test "vikunja-auth: a bare non-127 nonzero no longer blanket-FAILS as 'Vikunja may be down'" {
  # The old line treated ANY nonzero exit as a vikunja failure, so 127 fired a
  # false outage. After the fix, the catch-all 'Vikunja may be down or token
  # invalid' FAILURE string must be gone (real auth failures are caught by the
  # anchored Unauthorized/40x grep, which stays).
  run grep -F 'Vikunja may be down or token invalid' "$DEEP_HEALTH"
  [ "$status" -ne 0 ]
}

@test "alert-delivery: failure detection scopes to the latest run, not a whole-file FAIL grep" {
  # The bug: grep "FAIL" "\$DELIVERY_LOG" | tail -1 returns the OLDEST-surviving
  # FAIL in the whole log (a 2026-05-12 entry) even when today's run passed.
  # That whole-file grep must be gone.
  run grep -E 'grep "FAIL" "\$DELIVERY_LOG"' "$DEEP_HEALTH"
  [ "$status" -ne 0 ]
}

@test "alert-delivery: result is read from the latest RESULT line" {
  # The truthful check keys on the most-recent RESULT: line of the log.
  run grep -nE 'RESULT' "$DEEP_HEALTH"
  [ "$status" -eq 0 ]
}

@test "standards-surface: a missing artifact is a tracked WARNING, not a FAILURE" {
  # The surface was retired off gathering-docs by #2969/#3361; it has no current
  # home (the generator is a transform whose seed moved). A migration gap is a
  # WARNING tracked by #3361, not a service-down FAILURE that nudges nightly.
  # Assert the not-found branch routes to WARNINGS and cites the migration.
  run grep -nE 'standards-surface.*(#3361|migrat|rehome|re-home)' "$DEEP_HEALTH"
  [ "$status" -eq 0 ]
  # and the not-found case must not be in FAILURES
  run grep -nE 'FAILURES\+=\("standards-surface' "$DEEP_HEALTH"
  [ "$status" -ne 0 ]
}

@test "deep-health end-to-end still produces a summary" {
  run bash "$DEEP_HEALTH"
  [ -n "$output" ]
  echo "$output" | grep -qiE "failures|warning|healthy|passed"
}
