#!/bin/bash
# athena-make artifact-drift check (#3354, extended #3364) — THE generic drift
# guard (Kade's call: one diff at the generation boundary, not per-class
# alert rules). Regenerates every artifact from the live graph and diffs
# against the committed baselines: routes, openapi contract, dashboard.
# A route/field/contract entry vanishing = fail-loud here, in chorus-health
# or at deploy — never silent.
set -uo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)/services/athena-make"
BIN="${OWL_API_BIN:-$DIR/target/release/athena-make}"
[ -x "$BIN" ] || BIN="$HOME/.chorus/bin/athena-make"
FAIL=0
for class in Domain; do
  lc=$(echo "$class" | tr '[:upper:]' '[:lower:]')
  if ! "$BIN" generate --class "$class" | diff -u "$DIR/generated/routes-$lc.json" - ; then
    echo "DRIFT: generated routes for $class differ from committed baseline" >&2
    FAIL=1
  fi
  if ! "$BIN" generate-openapi --class "$class" | diff -u "$DIR/generated/openapi-$lc.json" - ; then
    echo "DRIFT: generated openapi contract for $class differs from committed baseline" >&2
    FAIL=1
  fi
  if ! "$BIN" generate-dashboard --class "$class" | diff -u "$DIR/generated/dashboard-$lc.json" - ; then
    echo "DRIFT: generated dashboard for $class differs from committed baseline" >&2
    FAIL=1
  fi
done
[ "$FAIL" -eq 0 ] && echo "athena-make artifacts: no drift"
exit $FAIL
