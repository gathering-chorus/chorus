#!/usr/bin/env bats
# @test-type: unit — runs the suites that hold #4887's AC; a headless load of the live room is AC1
# @domain: messages
# @card: #4887 · owner: silas
# What Jeff sees: no false "reply never reached Clearing" alert for a reply that is
# on the room's screen, and a real one when a reply truly is missing. The gap watch
# reads the room's own record, not a spine echo nobody writes any more.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# Jeff's public room, the page he loads (AC1 says the public room, not :3472).
ROOM_URL="${CLEARING_ROOM_URL:-https://lightlifeurbangardens.com/clearing-v2/}"
ROWS="$ROOT/platform/tests/lib/room-rows-4887.cjs"
PW="$ROOT/node_modules/playwright-core"

@test "AC1: the public room renders Abby's (agy) latest reply and the latest Claude-role reply" {
  run node "$ROWS" "$PW" "$ROOM_URL" abby-normal 'silas|wren|kade'
  echo "$output"
  # 3 = a latest reply the page has scrolled past: unmeasured, reported as a skip
  [ "$status" -ne 3 ] || skip "unmeasured: $output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC1 negative proof: a latest reply that is in the record but not on the page fails" {
  # the record holds both replies; the page shows only an older Abby row
  dir="$BATS_TEST_TMPDIR/room"; mkdir -p "$dir"
  printf '<div id="messages-stream"><div class="message" id="msg-old" data-from="abby-normal" data-ts="2026-11-04T18:00:00.000Z"></div></div>' > "$dir/index.html"
  printf '[{"id":"old","from":"abby-normal","text":"an old reply","ts":"2026-11-04T18:00:00.000Z"},{"id":"a1","from":"abby-normal","text":"her latest","ts":"2026-11-04T18:05:00.000Z"},{"id":"k1","from":"kade","text":"his latest","ts":"2026-11-04T18:06:00.000Z"}]' > "$dir/record.json"
  a="$(node -e 'console.log(require(process.argv[1]).contentHash("her latest"))' "$ROOT/platform/chorus-sdk/reply/hash.js")"
  k="$(node -e 'console.log(require(process.argv[1]).contentHash("his latest"))' "$ROOT/platform/chorus-sdk/reply/hash.js")"
  run env WANT_4887="abby-normal:$a,kade:$k" ROOM_RECORD_FILE="$dir/record.json" node "$ROWS" "$PW" "file://$dir/index.html"
  echo "$output"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *'"missing":[{"role":"abby-normal"'*'"role":"kade"'* ]] || return 1
  # control: the same page WITH her latest row passes for her
  printf '<div id="messages-stream"><div class="message" id="msg-a1" data-from="abby-normal" data-ts="2026-11-04T18:05:00.000Z"></div></div>' > "$dir/index.html"
  run env WANT_4887="abby-normal:$a" ROOM_RECORD_FILE="$dir/record.json" node "$ROWS" "$PW" "file://$dir/index.html"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC1: a latest reply older than the page's oldest row is unmeasured (exit 3), not red and not a pass" {
  dir="$BATS_TEST_TMPDIR/room"; mkdir -p "$dir"
  printf '<div id="messages-stream"><div class="message" id="msg-k1" data-from="kade" data-ts="2026-11-04T18:06:00.000Z"></div></div>' > "$dir/index.html"
  printf '[{"id":"a1","from":"abby-normal","text":"her latest","ts":"2026-11-04T18:05:00.000Z"},{"id":"k1","from":"kade","text":"his latest","ts":"2026-11-04T18:06:00.000Z"}]' > "$dir/record.json"
  a="$(node -e 'console.log(require(process.argv[1]).contentHash("her latest"))' "$ROOT/platform/chorus-sdk/reply/hash.js")"
  run env WANT_4887="abby-normal:$a" ROOM_RECORD_FILE="$dir/record.json" node "$ROWS" "$PW" "file://$dir/index.html"
  echo "$output"
  [ "$status" -eq 3 ] || return 1
  [[ "$output" == *'"scrolled":[{"role":"abby-normal"'* ]] || return 1
}

@test "AC2: Abby's 13:02:57 and 13:03:10 replies, in the room, count as delivered" {
  cd "$ROOT/platform/pulse"
  run npx jest src/reply-gap-4887.test.ts -t "AC2|one join key|fixture is whole|different role"
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
  # per-account target (#4885): each account builds into its own dir, never another's files
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="target/$(id -un)" nice cargo test --test reply_delivery_3864
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
