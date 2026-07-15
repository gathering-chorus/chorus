#!/usr/bin/env bash
# chorus-sdk-deploy.sh — #3628: the deploy verb chorus-sdk never had.
#
# chorus-sdk is a library, not a LaunchAgent: consumers (cards CLI via the
# node_modules symlink) load dist/ live on every invocation, so "deploy" =
# rebuild dist from landed src, verified, atomically. Before this script the
# SDK had NO deploy path — #3619's token-wired emit landed in src Jul 8 while
# dist stayed on its Jun 2 build and every consumer kept POSTing tokenless.
#
# Sequence: tsc into a temp dir → verify every src module has a counterpart
# (check-sdk-propagation's rule, applied pre-swap) → dist → dist.prev backup,
# temp → dist atomic move → spine event. Build failure leaves dist untouched.
#
# Overrides for tests: CHORUS_SDK_DIR (target package), CHORUS_SDK_BUILD_CMD
# (build command run via bash -c with CHORUS_SDK_BUILD_OUT set to the temp
# outDir; default is npx tsc --outDir into it).

set -euo pipefail

SDK_DIR="${CHORUS_SDK_DIR:-/Users/jeffbridwell/CascadeProjects/chorus/platform/chorus-sdk}"
CHORUS_LOG_BIN="${CHORUS_LOG_BIN:-$(command -v chorus-log || echo /Users/jeffbridwell/.chorus/scripts/chorus-log)}"

cd "$SDK_DIR"
TMP_OUT="$(mktemp -d "${TMPDIR:-/tmp}/chorus-sdk-build.XXXXXX")"
trap 'rm -rf "$TMP_OUT"' EXIT
export CHORUS_SDK_BUILD_OUT="$TMP_OUT"

build() {
  if [[ -n "${CHORUS_SDK_BUILD_CMD:-}" ]]; then
    bash -c "$CHORUS_SDK_BUILD_CMD"
  else
    npx tsc --outDir "$TMP_OUT"
  fi
}

echo "Building chorus-sdk → $TMP_OUT"
if ! build; then
  echo "ERROR: build failed — dist untouched"
  "$CHORUS_LOG_BIN" sdk.deploy.failed silas step=build 2>/dev/null || true
  exit 1
fi

# Pre-swap completeness: every runtime src module must have a built counterpart.
for src in src/*.ts; do
  mod="$(basename "$src" .ts)"
  [[ "$mod" == *.d ]] && continue
  if [[ ! -f "$TMP_OUT/$mod.js" ]]; then
    echo "ERROR: build output missing $mod.js — dist untouched"
    "$CHORUS_LOG_BIN" sdk.deploy.failed silas step=verify "missing=$mod.js" 2>/dev/null || true
    exit 1
  fi
done

rm -rf dist.prev
[[ -d dist ]] && mv dist dist.prev
mv "$TMP_OUT" dist
trap - EXIT

echo "Deployed chorus-sdk dist ($(ls dist/*.js | wc -l | tr -d ' ') modules); previous build at dist.prev"
"$CHORUS_LOG_BIN" sdk.deploy.completed silas "modules=$(ls dist/*.js | wc -l | tr -d ' ')" 2>/dev/null || true
