#!/usr/bin/env bash
# gate-code-tests.sh — run jest scoped to changed files (#2226)
#
# Replaces 'cd project && npm test' pattern in /gate-code. Uses
# jest --findRelatedTests to run only tests that depend on changed
# src files.
#
# Usage:
#   gate-code-tests.sh [BASE_REF] [--full]
#
#   BASE_REF  — git ref to diff against (default: HEAD~3 or WIP start)
#   --full    — force full suite, skip scoping
#
# Fall-through to full suite:
#   - --full flag explicitly given
#   - >20 changed .ts files (likely refactor, safety)
#   - 0 changed .ts files (nothing to scope; run full as safety)
#
# Demo pre-flight always runs full suite (DEC-048) — this script is
# for gate:code loop only. Demo continues to invoke `npm test` directly.
#
# Scoping limit: tests that don't import production code (e.g., pure
# integration tests hitting live services) won't be picked up by
# --findRelatedTests. Those are already quarantined via RUN_INTEGRATION
# in jest.config.js testPathIgnorePatterns, so they're safe — demo
# pre-flight is where they'd run. If a hermetic test has no import
# from src/ (unusual), it also won't get picked; use --full or
# GATE_CODE_FULL=true for those cases.
#
# Exit codes:
#   0 — all scoped (or full) runs passed
#   1 — at least one project's jest exited non-zero
#
# Silent when scope is empty: prints one-line summary, exits 0.

set -euo pipefail

REPO=/Users/jeffbridwell/CascadeProjects/chorus
BASE="HEAD~3"
FULL=0

# Parse args (positional BASE + --full flag in any order)
for arg in "$@"; do
  case "$arg" in
    --full)
      FULL=1
      ;;
    --*)
      echo "gate-code-tests: unknown flag: $arg" >&2
      exit 2
      ;;
    *)
      BASE="$arg"
      ;;
  esac
done

# Projects with jest.config.js — keep in sync with #2225
PROJECTS=(
  "platform/api"
  "platform/chorus-sdk"
  "platform/pulse"
  "platform/workflow-engine"
  "directing/clearing"
  "directing/products/cards"
)

cd "$REPO"

# Verify base ref exists; if not, fall back to HEAD~1 with a note
if ! git rev-parse --verify -q "$BASE" >/dev/null; then
  echo "gate-code-tests: base ref '$BASE' not resolvable, falling back to HEAD~1" >&2
  BASE="HEAD~1"
fi

# Get changed .ts files vs base (includes staged + committed, not working-tree)
CHANGED=$(git diff "$BASE" --name-only -- '*.ts' 2>/dev/null | grep -v '^$' || true)
CHANGED_COUNT=$(printf '%s\n' "$CHANGED" | grep -c . || true)

# Lint gate (#2288): run eslint with --max-warnings=0 before jest. Scoped to
# changed .ts files when possible; full repo pass on --full or refactor-sized
# diffs. Exits non-zero on any violation — regressions block at gate-code,
# not at demo.
if [ "$FULL" = "1" ] || [ "$CHANGED_COUNT" = "0" ] || [ "$CHANGED_COUNT" -gt 20 ]; then
  echo "--- lint (full) ---"
  if ! npm run lint --silent; then
    echo "gate-code-tests: lint failed (full)" >&2
    exit 1
  fi
else
  echo "--- lint ($CHANGED_COUNT changed .ts) ---"
  # shellcheck disable=SC2046
  if ! npx eslint --max-warnings=0 $(printf '%s ' $CHANGED); then
    echo "gate-code-tests: lint failed (scoped)" >&2
    exit 1
  fi
fi

# Decide full-fallback
run_full=0
reason=""
if [ "$FULL" = "1" ]; then
  run_full=1
  reason="--full flag"
elif [ "$CHANGED_COUNT" = "0" ]; then
  run_full=1
  reason="no .ts changes since $BASE — full suite as safety"
elif [ "$CHANGED_COUNT" -gt 20 ]; then
  run_full=1
  reason="$CHANGED_COUNT changed files > 20 threshold — likely refactor, full suite"
fi

if [ "$run_full" = "1" ]; then
  echo "gate-code-tests: $reason"
  exit_code=0
  for proj in "${PROJECTS[@]}"; do
    if [ ! -d "$REPO/$proj" ]; then continue; fi
    echo "--- $proj (full suite) ---"
    if ! (cd "$REPO/$proj" && npx jest); then
      exit_code=1
    fi
  done
  exit $exit_code
fi

# Scoped mode: per-project, only projects with changes get a scoped run
echo "gate-code-tests: scoping $CHANGED_COUNT changed file(s) since $BASE"
exit_code=0
any_scoped=0
for proj in "${PROJECTS[@]}"; do
  if [ ! -d "$REPO/$proj" ]; then continue; fi
  # Filter CHANGED to files inside this project
  PROJ_CHANGED=$(printf '%s\n' "$CHANGED" | grep "^$proj/" || true)
  if [ -z "$PROJ_CHANGED" ]; then continue; fi
  # Strip prefix for jest --findRelatedTests (expects project-relative)
  REL=$(printf '%s\n' "$PROJ_CHANGED" | sed "s|^$proj/||")
  REL_COUNT=$(printf '%s\n' "$REL" | grep -c . || true)
  echo "--- $proj ($REL_COUNT changed .ts) ---"
  # jest --findRelatedTests: maps src files to their dependent tests
  # Passing test files directly also works (jest runs them)
  any_scoped=1
  # shellcheck disable=SC2046
  if ! (cd "$REPO/$proj" && npx jest --findRelatedTests $(printf '%s ' $REL)); then
    exit_code=1
  fi
done

if [ "$any_scoped" = "0" ]; then
  echo "gate-code-tests: $CHANGED_COUNT .ts change(s), but none in projects with jest configs"
fi

exit $exit_code
