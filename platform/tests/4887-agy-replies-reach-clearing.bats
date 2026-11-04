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
  fixture="$ROOT/config/reply-hash-fixtures.json"
  n="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["cases"]))' "$fixture")"
  [ "$n" -gt 0 ] || { echo "the shared fixture has no cases"; return 1; }
  cd "$ROOT/platform/chorus-sdk"
  run npx jest tests/reply-hash-4887.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  cd "$ROOT/platform/services/chorus-hooks"
  run env CARGO_BUILD_JOBS=2 nice cargo test --test reply_delivery_3864
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

# AC4 (Jeff 13:10): no new publisher, relay or fallback. Reads a unified diff on
# stdin, prints every ADDED line that calls a reply publisher or posts a message.
added_publishers() {
  grep -E '^\+[^+]' | grep -E "deliver_and_emit\(|emit_and_deliver_reply\(|publish_reply|nudge-send|/api/nudge|method: *['\"]POST['\"]|\.post\(|appendFile\(" || true
}

@test "AC4: the card's diff adds no publish, relay or fallback caller" {
  cd "$ROOT"
  base="$(git merge-base HEAD origin/main)"
  out="$(git diff "$base" -- . ':!platform/tests/4887-agy-replies-reach-clearing.bats' ':!*.test.ts' | added_publishers)"
  [ -z "$out" ] || { echo "added publisher: $out"; return 1; }
  # and the one reply publisher still has exactly one caller (main.rs emit_and_deliver_reply)
  n="$(grep -rn 'deliver_and_emit(' platform/services/chorus-hooks/src | grep -v 'pub async fn' | wc -l | tr -d ' ')"
  [ "$n" -eq 1 ] || { echo "deliver_and_emit callers: $n"; return 1; }
}

@test "AC4 negative proof: a planted second caller is caught" {
  planted=$'+++ b/platform/services/chorus-hooks/src/main.rs\n+    hooks::buzz_reply::deliver_and_emit(role, text, &hash).await;\n+  await fetch(url, { method: \'POST\', body });'
  out="$(printf '%s\n' "$planted" | added_publishers)"
  [[ "$out" == *"deliver_and_emit("* ]] || { echo "missed the planted caller: $out"; return 1; }
  [[ "$out" == *"method: 'POST'"* ]] || { echo "missed the planted POST: $out"; return 1; }
}

# AC6: code lines (not tests, not comments) that name reply.rendered under the given dirs.
rendered_refs() {
  grep -rn 'reply.rendered' "$@" --include='*.ts' | grep -v '\.test\.ts:' | grep -vE '^\S+:[0-9]+:\s*(//|\*)' || true
}

@test "AC6: the reply.rendered echo is retired — no writer, no reader" {
  cd "$ROOT"
  out="$(rendered_refs directing/clearing/src platform/pulse/src)"
  [ -z "$out" ] || { echo "$out"; return 1; }
  # and the v1 emitter itself is gone, not just unused
  out="$(grep -rn 'makeSpineEmitter\|renderedEvent' directing/clearing/src || true)"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC6 negative proof: a planted reply.rendered writer is caught, a comment is not" {
  mkdir -p "$BATS_TEST_TMPDIR/src"
  printf "  // reply.rendered retired\n  emit({ event: 'reply.rendered', role });\n" > "$BATS_TEST_TMPDIR/src/x.ts"
  out="$(rendered_refs "$BATS_TEST_TMPDIR/src")"
  [[ "$out" == *"emit({ event: 'reply.rendered'"* ]] || { echo "missed: $out"; return 1; }
  [[ "$out" != *"// reply.rendered"* ]] || { echo "counted a comment: $out"; return 1; }
}
