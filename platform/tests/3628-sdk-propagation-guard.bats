#!/usr/bin/env bats
# @test-type: unit
# #3628 AC4 — deploy-completeness guard, red-first per DEC-1674.
#
# The failure class this pins: #3619 landed token-wired src/emit.ts +
# src/token.ts on Jul 8, but chorus-sdk/dist (untracked build artifact) was
# last built Jun 2 — dist/token.js never existed, every SDK consumer kept
# POSTing tokenless, and /api/chorus/trace refused 4354 real calls/24h with
# nobody told. The guard fails loud when a landed chorus-sdk change has not
# been built into the dist its consumers actually load.
#
# The test brings its own world: fixture SDK dirs under $BATS_TEST_TMPDIR,
# CHORUS_SDK_DIR points the script at them. No live ~/.chorus, no real repo.

REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/check-sdk-propagation.sh"

make_sdk() { # $1 = fixture root; creates src/{emit,token}.ts + dist/{emit,token}.js
  mkdir -p "$1/src" "$1/dist"
  echo "// ts" > "$1/src/emit.ts"
  echo "// ts" > "$1/src/token.ts"
  echo "// js" > "$1/dist/emit.js"
  echo "// js" > "$1/dist/token.js"
}

@test "guard script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "fresh dist (built after newest src) → ok, exit 0" {
  SDK="$BATS_TEST_TMPDIR/fresh"; make_sdk "$SDK"
  touch -t 202601010000 "$SDK/src/emit.ts" "$SDK/src/token.ts"
  touch -t 202601020000 "$SDK/dist/emit.js" "$SDK/dist/token.js"
  run env CHORUS_SDK_DIR="$SDK" "$SCRIPT"
  [ "$status" -eq 0 ]
  [ "$output" = "ok" ]
}

@test "src newer than dist → FAIL, exit 1, names the stale artifact" {
  SDK="$BATS_TEST_TMPDIR/stale"; make_sdk "$SDK"
  touch -t 202601020000 "$SDK/src/emit.ts"
  touch -t 202601010000 "$SDK/dist/emit.js" "$SDK/dist/token.js" "$SDK/src/token.ts"
  run env CHORUS_SDK_DIR="$SDK" "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"stale"* ]]
  [[ "$output" == *"emit"* ]]
}

@test "src module with NO dist counterpart → FAIL (the token.js miss)" {
  SDK="$BATS_TEST_TMPDIR/missing"; make_sdk "$SDK"
  rm "$SDK/dist/token.js"
  # dist newer than src — mtime check alone would pass; the counterpart check must catch it
  touch -t 202601010000 "$SDK/src/emit.ts" "$SDK/src/token.ts"
  touch -t 202601020000 "$SDK/dist/emit.js"
  run env CHORUS_SDK_DIR="$SDK" "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"token"* ]]
}

@test "dist directory absent entirely → FAIL, never a bash error" {
  SDK="$BATS_TEST_TMPDIR/nodist"; mkdir -p "$SDK/src"
  echo "// ts" > "$SDK/src/emit.ts"
  run env CHORUS_SDK_DIR="$SDK" "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"dist"* ]]
}
