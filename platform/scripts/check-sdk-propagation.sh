#!/usr/bin/env bash
# check-sdk-propagation.sh — #3628 AC4: deploy-completeness guard for chorus-sdk.
#
# chorus-sdk/dist is an UNTRACKED build artifact loaded live by every consumer
# (cards CLI resolves node_modules/chorus-sdk → platform/chorus-sdk/dist). A
# landed src change that is never rebuilt ships nothing: #3619's token-wired
# emit sat in src for 5 weeks while the Jun 2 dist kept POSTing tokenless and
# /api/chorus/trace refused 4354 real calls/24h. This guard makes that state
# loud instead of silent.
#
# Checks (stdout "ok" + exit 0 when healthy; FAIL lines + exit 1 otherwise):
#   1. dist/ exists
#   2. every src/<mod>.ts has a dist/<mod>.js counterpart (catches a module
#      added to src — token.ts — that a stale dist predates)
#   3. no src/<mod>.ts is newer than its dist/<mod>.js (catches an edited
#      module whose rebuild never ran)
#
# CHORUS_SDK_DIR overrides the target for tests — the test brings its own
# world; this script never touches live state and mutates nothing.

set -euo pipefail

SDK_DIR="${CHORUS_SDK_DIR:-/Users/jeffbridwell/CascadeProjects/chorus/platform/chorus-sdk}"
SRC_DIR="$SDK_DIR/src"
DIST_DIR="$SDK_DIR/dist"

fail=0

if [[ ! -d "$DIST_DIR" ]]; then
  echo "FAIL: $DIST_DIR missing — chorus-sdk has never been built here"
  exit 1
fi

shopt -s nullglob
for src in "$SRC_DIR"/*.ts; do
  mod="$(basename "$src" .ts)"
  # declaration-only sources have no runtime counterpart
  [[ "$mod" == *.d ]] && continue
  dist="$DIST_DIR/$mod.js"
  if [[ ! -f "$dist" ]]; then
    echo "FAIL: src/$mod.ts has no dist/$mod.js — dist predates this module; rebuild chorus-sdk"
    fail=1
    continue
  fi
  if [[ "$src" -nt "$dist" ]]; then
    echo "FAIL: dist/$mod.js is stale — src/$mod.ts is newer; rebuild chorus-sdk"
    fail=1
  fi
done

if [[ $fail -eq 1 ]]; then
  exit 1
fi

echo "ok"
