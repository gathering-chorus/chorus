#!/usr/bin/env bats
# @test-type: unit — Loki's own frontend stats lines from 15:32 on 2026-11-15 (a fixture), and the card's AC1 note
# @domain: logs
# @card: #4983 · owner: silas
# Checks platform/scripts/loki-fanout and platform/tests/fixtures/4983/loki-frontend-1532.log.
#
# #4983 AC1: the cause of the 11-15 refusals is named from Loki's own log lines and metrics, on the
# card, before any setting changes. The cause: one 24h read fans out to 784 queued sub-queries, and
# Loki's per-tenant queue (max_outstanding_requests_per_tenant) holds 100, so every other request in
# that window is refused 429.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  FIX="$ROOT/platform/tests/fixtures/4983/loki-frontend-1532.log"
}

# the AC1 note names the window, the fan-out, the queue and the refusal: true when one card item does
cause_named() {
  python3 -c '
import json, sys
items = json.load(sys.stdin).get("timeline", [])
need = ("15:32", "784", "max_outstanding_requests_per_tenant 100", "429")
sys.exit(0 if any(all(n in (i.get("text") or "") for n in need) for i in items) else 1)'
}

@test "the 15:32 reads overflow a queue of 100: 784 sub-queries from one 24h read" {
  run "$ROOT/platform/scripts/loki-fanout" --queue 100 < "$FIX"
  [ "$status" -eq 1 ] && [[ "$output" == *"overflow: 784 sub-queries from one read > queue 100"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"length=24h0m0s"* ]] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: the same reads against a queue that holds them is not an overflow
@test "the same reads fit a queue of 1000; no stats lines measure nothing" {
  run "$ROOT/platform/scripts/loki-fanout" --queue 1000 < "$FIX"
  [ "$status" -eq 0 ] && [[ "$output" == *"fits: the largest read is 784"* ]] || { echo "$output"; return 1; }
  run "$ROOT/platform/scripts/loki-fanout" --queue 100 < /dev/null
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "the card names the cause; a note without it does not pass" {
  # the card's story from the Chorus API (comments included); a role account has no board token
  card="$(curl -sf --max-time 15 "${CHORUS_API:-http://localhost:3340}/api/chorus/card-story/4983")" || { echo "card-story unreadable"; return 1; }
  printf '%s' "$card" | cause_named || { echo "no card item names the cause"; return 1; }
  # NEGATIVE PROOF: the same story with the cause items taken out fails
  printf '%s' "$card" | python3 -c '
import json, sys
d = json.load(sys.stdin)
d["timeline"] = [i for i in d.get("timeline", []) if "784" not in (i.get("text") or "")]
print(json.dumps(d))' | cause_named && { echo "a story without the cause item still passed"; return 1; }
  return 0
}
