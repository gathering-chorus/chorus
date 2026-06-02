#!/usr/bin/env bats
# #3181 — chorus-deploy must ff canonical to origin/main BEFORE building (or refuse
# when it can't ff cleanly), so a "merged" fix is never built from a stale canonical.
# This was the merged≠live engine: canonical sat 13 behind all of 2026-06-01 and
# chorus-deploy (which builds from canonical HEAD) shipped pre-rename code green.
#
# Harness mirrors ac4-chorus-deploy-rollback.bats: real git fixtures + PATH-stubbed
# build machinery so the run reaches (and exercises) the guard in isolation.

DEPLOY_SCRIPT="${DEPLOY_SCRIPT:-$BATS_TEST_DIRNAME/../scripts/chorus-deploy}"

gitc() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git "$@"; }

# Make $2 a clone of a 2-commit origin, reset 1 behind → behind origin/main by 1, clean.
make_behind_clone() {
  local origin="$1" canon="$2"
  git init -q -b main "$origin"
  ( cd "$origin"; echo a > f; gitc add -A; gitc commit -q -m c1; echo b >> f; gitc add -A; gitc commit -q -m c2 )
  git clone -q "$origin" "$canon"
  ( cd "$canon"; gitc reset -q --hard HEAD~1 )   # local HEAD=c1, origin/main=c2
}

setup() {
  BASE=$(mktemp -d -t ac3181.XXXXXX)
  ORIGIN="$BASE/origin"
  CANON="$BASE/canon"
  STUBDIR="$BASE/stubs"
  mkdir -p "$STUBDIR"
  make_behind_clone "$ORIGIN" "$CANON"

  # Stub every build/deploy tool to a no-op success, so reaching past the guard
  # never touches real cargo/npm/launchd/codesign.
  for cmd in cargo npm launchctl codesign chorus-bin-install build-signed.sh cards chorus-log rsync; do
    printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/$cmd"
    chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  export CHORUS_ROOT="$CANON"
}

teardown() { rm -rf "$BASE"; }

@test "ff: fast-forwards a cleanly-behind canonical before building" {
  run "$DEPLOY_SCRIPT" chorus-mcp
  # ff is observable regardless of the downstream build's outcome
  [ "$(git -C "$CANON" rev-parse HEAD)" = "$(git -C "$CANON" rev-parse origin/main)" ]
  [[ "$output" == *"fast-forwarding before build"* ]]
}

@test "refuse: dirty + behind canonical → REFUSE, no ff, no build" {
  echo dirty > "$CANON/uncommitted.txt"
  before="$(git -C "$CANON" rev-parse HEAD)"
  run "$DEPLOY_SCRIPT" chorus-mcp
  [ "$status" -eq 1 ]
  [ "$(git -C "$CANON" rev-parse HEAD)" = "$before" ]   # not advanced
  # discriminator LAST (bats only binds the final command): unguarded prints a
  # build error, never this guard message.
  [[ "$output" == *"can't ff cleanly"* ]]
}

@test "refuse: diverged (ahead + behind) canonical → REFUSE" {
  ( cd "$CANON"; echo x > y; gitc add -A; gitc commit -q -m "local ahead" )
  run "$DEPLOY_SCRIPT" chorus-mcp
  [ "$status" -eq 1 ]
  [[ "$output" == *"can't ff cleanly"* ]]
}

@test "werk-sourced build (ROOT under chorus-werk) skips canonical ff even when behind" {
  WERK="$BASE/chorus-werk/silas-9999"
  mkdir -p "$BASE/chorus-werk"
  make_behind_clone "$BASE/werkorigin" "$WERK"
  export CHORUS_ROOT="$WERK"
  before="$(git -C "$WERK" rev-parse HEAD)"
  run "$DEPLOY_SCRIPT" chorus-mcp
  [ "$(git -C "$WERK" rev-parse HEAD)" = "$before" ]    # no ff attempted
  [[ "$output" != *"fast-forwarding before build"* ]]
}

@test "no-git canonical: guard no-ops (preserves existing chorus-deploy tests)" {
  NOGIT="$BASE/nogit"; mkdir -p "$NOGIT"
  export CHORUS_ROOT="$NOGIT"
  run "$DEPLOY_SCRIPT" chorus-mcp
  [[ "$output" != *"fast-forwarding"* ]]
  [[ "$output" != *"REFUSING"* ]]
}
