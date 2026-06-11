#!/bin/bash
# owl-api artifact-drift check (#3354) — THE generic drift guard (Kade's call:
# one diff at the generation boundary, not per-class alert rules).
# Regenerates the artifacts from the live graph and diffs against the
# committed baselines. A route/field vanishing = fail-loud here, in CI or at
# deploy — never silent.
set -uo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)/services/owl-api"
BIN="${OWL_API_BIN:-$DIR/target/release/owl-api}"
FAIL=0
for class in Domain; do
  lc=$(echo "$class" | tr '[:upper:]' '[:lower:]')
  if ! "$BIN" generate --class "$class" | diff -u "$DIR/generated/routes-$lc.json" - ; then
    echo "DRIFT: generated routes for $class differ from committed baseline" >&2
    FAIL=1
  fi
done
[ "$FAIL" -eq 0 ] && echo "owl-api artifacts: no drift"
exit $FAIL
