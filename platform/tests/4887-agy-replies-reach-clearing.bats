#!/usr/bin/env bats
# @test-type: unit — runs the suites that hold #4887's AC; a headless load of the live room is AC1
# @domain: messages
# @card: #4887 · owner: silas
# What Jeff sees: no false "reply never reached Clearing" alert for a reply that is
# on the room's screen, and a real one when a reply truly is missing. The gap watch
# reads the room's own record, not a spine echo nobody writes any more.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC2: Abby's 13:02:57 and 13:03:10 replies, in the room, count as delivered" {
  cd "$ROOT/platform/pulse"
  run npx jest src/reply-gap-4887.test.ts -t "AC2|one join key|fixture is whole"
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

@test "AC3: negative proof — a reply missing from the room still raises a gap" {
  cd "$ROOT/platform/pulse"
  run npx jest src/reply-gap-4887.test.ts -t "AC3"
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

@test "an unreadable room, or a reply the buffer has scrolled past, reads unmeasured, never a gap" {
  cd "$ROOT/platform/pulse"
  run npx jest src/reply-gap-4887.test.ts -t "unmeasured"
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

@test "AC5: one join key — Rust content_hash and TS contentHash agree on the shared fixture" {
  cd "$ROOT/platform/chorus-sdk"
  run npx jest tests/reply-hash-4887.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  cd "$ROOT/platform/services/chorus-hooks"
  run env CARGO_BUILD_JOBS=2 nice cargo test --test reply_delivery_3864
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

@test "AC4+AC6: no reply.rendered writer or reader is left, and no new publisher" {
  cd "$ROOT"
  # the echo is retired: nothing writes or reads reply.rendered outside tests and comments
  run bash -c "grep -rn \"'reply.rendered'\" directing/clearing/src platform/pulse/src --include=*.ts | grep -v '\.test\.ts:'"
  [ -z "$output" ] || { echo "$output"; return 1; }
  [ ! -e directing/clearing/src/reply-delivery.ts ] || { echo "reply-delivery.ts still present"; return 1; }
  # the watch reads the room, it does not post anywhere new
  run grep -nE "fetch\([^)]*(POST|method: *'POST')|appendFile|publish" platform/pulse/src/reply-gap.ts
  [ -z "$output" ] || { echo "$output"; return 1; }
}
