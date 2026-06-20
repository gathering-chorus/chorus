#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# test-manifest-writer.bats — TDD for #2791 manifest writer + chorus-manifest CLI.
#
# Red-phase: all tests fail before chorus-manifest CLI exists.
#
# Tests the AC scenarios:
#   1. Idempotency by cdhash (same commit + crate → single entry)
#   2. Different commits → two entries
#   3. chorus-manifest get returns correct entry by {crate, commit}
#   4. chorus-manifest verify PASSES after install
#   5. Synthesized cdhash mismatch → verify exits 1 with diff
#   6. chorus-manifest rebuild-from-spine round-trips from log alone
#
# Tests use CHORUS_MANIFEST_PATH override + chorus-log stub.

CHORUS_MANIFEST="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}/platform/scripts/chorus-manifest"
[ -f "$CHORUS_MANIFEST" ] || CHORUS_MANIFEST="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/chorus-manifest"

setup() {
  TEST_HOME=$(mktemp -d)
  export CHORUS_MANIFEST_PATH="$TEST_HOME/.chorus/manifest.json"
  export CHORUS_BIN_DIR="$TEST_HOME/.chorus/bin"
  export CHORUS_TEST_SPINE_LOG="$TEST_HOME/.spine-events.log"
  mkdir -p "$(dirname "$CHORUS_MANIFEST_PATH")" "$CHORUS_BIN_DIR"
}

teardown() {
  rm -rf "$TEST_HOME"
}

# Helper: stable test fixture values
COMMIT_A="abc1234"
COMMIT_B="def5678"
CRATE="chorus-hooks"
IDENTIFIER="chorus-hook-shim"
CDHASH_1="0123456789abcdef0123456789abcdef01234567"
CDHASH_2="fedcba9876543210fedcba9876543210fedcba98"
SHA256_1="$(printf '%064s' '' | tr ' ' '0')"
SHA256_2="$(printf '%064s' '' | tr ' ' '1')"
BUILD_TIME="2026-05-07T22:00:00Z"
BUILDER_HOST="test-host"
BUILDER_ROLE="silas"

# AC: idempotency by cdhash — same {commit, crate, identifier, cdhash} added twice → 1 entry
@test "idempotency: same cdhash added twice yields single entry" {
  run "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE"
  [ "$status" -eq 0 ]
  run "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE"
  [ "$status" -eq 0 ]

  # Single entry expected
  run "$CHORUS_MANIFEST" list
  [ "$status" -eq 0 ]
  count=$(echo "$output" | python3 -c 'import sys, json; print(len(json.load(sys.stdin)))')
  [ "$count" -eq 1 ]
}

# AC: idempotency does NOT re-emit spine event
@test "idempotency: second add does not emit a second spine event" {
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  # Spine log should have exactly 1 manifest.entry.added line
  count=$(grep -c "manifest.entry.added" "$CHORUS_TEST_SPINE_LOG" 2>/dev/null || echo 0)
  [ "$count" -eq 1 ]
}

# AC: different commits → two entries
@test "two commits yield two entries" {
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null
  "$CHORUS_MANIFEST" add "$COMMIT_B" "$CRATE" "$IDENTIFIER" "$CDHASH_2" "$SHA256_2" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  run "$CHORUS_MANIFEST" list
  [ "$status" -eq 0 ]
  count=$(echo "$output" | python3 -c 'import sys, json; print(len(json.load(sys.stdin)))')
  [ "$count" -eq 2 ]
}

# AC: chorus-manifest get <crate> --commit <sha> returns the entry
@test "get returns entry by commit + crate" {
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  run "$CHORUS_MANIFEST" get "$CRATE" --commit "$COMMIT_A"
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['commit'] == '$COMMIT_A', d
assert d['crate'] == '$CRATE', d
assert d['cdhash'] == '$CDHASH_1', d
"
}

# AC: get returns not-found exit 2 when entry missing
@test "get returns not-found for missing entry" {
  run "$CHORUS_MANIFEST" get "$CRATE" --commit "deadbeef"
  [ "$status" -eq 2 ]
  [[ "$output" =~ "not-found" || "$stderr" =~ "not-found" ]]
}

# AC: verify PASSES when manifest cdhash matches deployed binary cdhash
@test "verify passes when manifest matches deployed binary" {
  # Stage a fake "deployed" binary with a known cdhash
  echo "fake-binary-content" > "$CHORUS_BIN_DIR/$IDENTIFIER"
  chmod +x "$CHORUS_BIN_DIR/$IDENTIFIER"
  # codesign the fake binary so it has a real cdhash, then add manifest entry with that cdhash
  # (skipped if codesign not available — verify becomes best-effort)
  if command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - "$CHORUS_BIN_DIR/$IDENTIFIER" 2>/dev/null || skip "codesign not usable on test binary"
    REAL_CDHASH=$(codesign -dvvv "$CHORUS_BIN_DIR/$IDENTIFIER" 2>&1 | grep '^CDHash=' | head -1 | sed 's/^CDHash=//')
    [ -n "$REAL_CDHASH" ] || skip "cdhash extraction failed"
  else
    skip "codesign not available"
  fi

  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$REAL_CDHASH" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  run "$CHORUS_MANIFEST" verify
  [ "$status" -eq 0 ]
}

# AC: verify exits 1 with diff on synthesized cdhash mismatch
@test "verify exits 1 on cdhash mismatch" {
  # Stage a fake binary
  echo "fake-binary-content" > "$CHORUS_BIN_DIR/$IDENTIFIER"
  chmod +x "$CHORUS_BIN_DIR/$IDENTIFIER"

  # Manifest claims a different cdhash than what's installed
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  run "$CHORUS_MANIFEST" verify
  [ "$status" -eq 1 ]
  [[ "$output" =~ "$IDENTIFIER" || "$output" =~ "mismatch" || "$output" =~ "drift" ]]
}

# Validation: malformed cdhash refused (Kade #2 — schema validation real, not aspirational)
@test "add refuses malformed cdhash" {
  run "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "not-hex-not-40-chars" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "cdhash" || "$stderr" =~ "cdhash" ]]
}

@test "add refuses unknown builder_role" {
  run "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "stranger"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "builder_role" || "$stderr" =~ "builder_role" ]]
}

@test "add refuses malformed build_time" {
  run "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "yesterday" "$BUILDER_HOST" "$BUILDER_ROLE"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "build_time" || "$stderr" =~ "build_time" ]]
}

# AC: rebuild-from-spine reproduces equivalent manifest from log alone
@test "rebuild-from-spine reproduces manifest from spine log" {
  # Add an entry which writes a spine event
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null
  "$CHORUS_MANIFEST" add "$COMMIT_B" "$CRATE" "$IDENTIFIER" "$CDHASH_2" "$SHA256_2" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  # Wipe the manifest, keeping only the spine log
  rm "$CHORUS_MANIFEST_PATH"

  # Rebuild from spine
  run "$CHORUS_MANIFEST" rebuild-from-spine "$CHORUS_TEST_SPINE_LOG"
  [ "$status" -eq 0 ]

  # Should have 2 entries again
  run "$CHORUS_MANIFEST" list
  [ "$status" -eq 0 ]
  count=$(echo "$output" | python3 -c 'import sys, json; print(len(json.load(sys.stdin)))')
  [ "$count" -eq 2 ]

  # Both cdhashes present
  echo "$output" | grep -q "$CDHASH_1"
  echo "$output" | grep -q "$CDHASH_2"
}

# AC: rebuild-from-spine deterministic — same log → byte-identical output (Kade #3)
@test "rebuild-from-spine produces byte-identical output across runs" {
  "$CHORUS_MANIFEST" add "$COMMIT_A" "$CRATE" "$IDENTIFIER" "$CDHASH_1" "$SHA256_1" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null
  "$CHORUS_MANIFEST" add "$COMMIT_B" "$CRATE" "$IDENTIFIER" "$CDHASH_2" "$SHA256_2" "$BUILD_TIME" "$BUILDER_HOST" "$BUILDER_ROLE" >/dev/null

  # First rebuild — capture output
  rm "$CHORUS_MANIFEST_PATH"
  "$CHORUS_MANIFEST" rebuild-from-spine "$CHORUS_TEST_SPINE_LOG" >/dev/null
  cp "$CHORUS_MANIFEST_PATH" "$TEST_HOME/rebuild-1.json"

  # Second rebuild — same log
  rm "$CHORUS_MANIFEST_PATH"
  "$CHORUS_MANIFEST" rebuild-from-spine "$CHORUS_TEST_SPINE_LOG" >/dev/null

  # Byte-identical
  run diff "$TEST_HOME/rebuild-1.json" "$CHORUS_MANIFEST_PATH"
  [ "$status" -eq 0 ]
}
