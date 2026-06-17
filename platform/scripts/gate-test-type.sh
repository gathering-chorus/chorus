#!/usr/bin/env bash
# #3442 — diff-scoped test-type gate. Enforces that every CHANGED test file
# carries a valid `@test-type` declaration consistent with its content signals.
#
# DIFF-SCOPED BY CONSTRUCTION: this script only ever feeds the gate the files in
# the current change — staged files (pre-commit) or the PR diff (CI). It never
# scans the whole corpus, so it can never become a "all 404 files must declare"
# mandate. New/changed tests declare as they're touched; the corpus migrates
# organically.
#
# Usage:
#   gate-test-type.sh staged              # pre-commit: git diff --cached
#   gate-test-type.sh range <base> <head> # CI: git diff base..head
set -euo pipefail

MODE="${1:-staged}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# NOTE: no `mapfile` — it's bash 4+, absent on macOS's default bash 3.2 (where
# it silently leaves CHANGED empty and the gate would never fire). Portable
# while-read into an array works on 3.2.
CHANGED=()
case "$MODE" in
  staged)
    while IFS= read -r f; do [ -n "$f" ] && CHANGED+=("$f"); done \
      < <(git diff --cached --name-only --diff-filter=ACM) ;;
  range)
    BASE="${2:?range mode needs <base>}"; HEAD="${3:?range mode needs <head>}"
    while IFS= read -r f; do [ -n "$f" ] && CHANGED+=("$f"); done \
      < <(git diff --name-only --diff-filter=ACM "$BASE" "$HEAD") ;;
  *)
    echo "usage: gate-test-type.sh staged | range <base> <head>" >&2; exit 2 ;;
esac

# Hand ALL changed paths to the CLI; isTestFile() inside filters to test files,
# so a change with no test files is a clean no-op (exit 0).
if [ "${#CHANGED[@]}" -eq 0 ]; then exit 0; fi

# FAIL-OPEN on a dev-setup gap, FAIL-CLOSED on a real violation — mirrors the
# gitleaks tool-missing convention (pre-commit). If the runtime is absent we
# SKIP (exit 0) rather than block work; CI gates hard (ADR-026).
if ! command -v npx >/dev/null 2>&1; then
  echo "gate-test-type: npx/node not available — test-type gate SKIPPED" >&2
  exit 0
fi

# tsx-on-source (consistent with CI's npx TS; avoids the stale-dist class that
# caused the dist self-loop board outage — no prebuilt dist for the hook). The
# CLI exits 0 on clean, 1 ONLY on a real declaration violation.
#
# Robust fail-open: block ONLY on exit code exactly 1 (a real violation). Any
# other non-zero (tsx not installed in this werk, runtime crash, etc.) is an
# infra gap, not a violation — SKIP rather than block work; CI/nightly gates the
# class harder. This makes the team-wide hook safe even where deps aren't bootstrapped.
set +e
(cd "$ROOT/platform/api" && npx tsx src/gate-test-type-cli.ts "${CHANGED[@]/#/$ROOT/}")
code=$?
set -e
if [ "$code" -eq 1 ]; then exit 1; fi
if [ "$code" -ne 0 ]; then
  echo "gate-test-type: runner unavailable (exit $code) — test-type gate SKIPPED" >&2
fi
exit 0
