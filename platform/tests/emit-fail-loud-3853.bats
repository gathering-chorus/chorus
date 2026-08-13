#!/usr/bin/env bats
# @test-type: unit
# #3853 — the spine emit must FAIL LOUD, never swallow. Negative proof (#3734):
# point the emit at an unwritable target and prove it exits non-zero. Before the
# fix, `let _ = f.write_all(...)` discarded the error and returned SUCCESS — a
# dropped emit reported success. This fixture is that exact violation state, and
# the emit is shown to now refuse it.

setup() {
  SHIM="${BATS_TEST_DIRNAME}/../services/chorus-hooks/target/debug/chorus-hook-shim"
  TMP="$(mktemp -d)"
  # a regular file where a directory is needed -> create_dir_all + open both fail
  touch "$TMP/blocker"
}
teardown() { rm -rf "$TMP"; }

@test "shim binary exists (built)" {
  [ -x "$SHIM" ]
}

@test "unwritable spine target exits NON-ZERO (fail loud), never silent success" {
  run env CI=1 CHORUS_LOG_FILE="$TMP/blocker/spine.jsonl" "$SHIM" log test.emit.failloud silas --level=info
  [ "$status" -ne 0 ]
  echo "$output" | grep -qiE "FAILED to (write|open)"
}

@test "writable spine target still succeeds and records the event" {
  run env CI=1 CHORUS_LOG_FILE="$TMP/ok/spine.jsonl" "$SHIM" log test.emit.ok silas --level=info
  [ "$status" -eq 0 ]
  grep -q 'test.emit.ok' "$TMP/ok/spine.jsonl"
}
