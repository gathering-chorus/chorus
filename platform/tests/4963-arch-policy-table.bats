#!/usr/bin/env bats
# @test-type: contract — the page and the route's handler; no server, no store.
# @domain: policies
# @card: #4963
# @owner: silas
#
# #4963 AC4 — the demo shows one table of the 13 with today's count each, worst first, on a
# page Jeff can open from his phone (not localhost): loom/policy-counts.html, served by
# chorus-api from disk, reads /api/chorus/policies/counts same-origin. The handler's own
# behaviour (worst first, narrowed marked, door rows only, 503 on a dead door, could-not-
# measure never shown as 0) is policy-counts-4963.test.ts, run here. The live answer on the
# LAN after the land is 4963-arch-policy-table-live.bats.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PAGE="$ROOT/platform/api/public/loom/policy-counts.html"

@test "the page reads the counts same-origin, never from localhost or another port" {
  grep -q "fetch('/api/chorus/policies/counts?gate=" "$PAGE" || return 1
  ! grep -qE "localhost|127\.0\.0\.1|:33[0-9]{2}|:3360" "$PAGE" || { grep -nE "localhost|127\.0\.0\.1|:33[0-9]{2}" "$PAGE"; return 1; }
}

# boston_time <page> — the page shows times through the shared Boston clock (#4965: domain-flow-time
# moved it off its own toLocaleString): it loads chorus-time/boston.js and calls bostonMinute
boston_time() {
  grep -q '<script src="/vendor/chorus-time/boston.js"></script>' "$1" || { echo "the page does not load chorus-time/boston.js"; return 1; }
  grep -q 'chorusBoston.bostonMinute(' "$1" || { echo "the page does not format with chorusBoston.bostonMinute"; return 1; }
}

@test "the page fits a phone and names each rule's narrowing and its ceiling" {
  grep -q 'name="viewport" content="width=device-width' "$PAGE" || return 1
  grep -q "narrowed" "$PAGE" && grep -q "ceiling" "$PAGE" && grep -q "could not measure" "$PAGE" || return 1
  boston_time "$PAGE" || return 1
}

@test "NEGATIVE PROOF: a page that formats its own time, or never loads the Boston clock, is refused" {
  own="$BATS_TEST_TMPDIR/own.html"; noscript="$BATS_TEST_TMPDIR/noscript.html"
  sed 's|chorusBoston.bostonMinute(new Date(b.measuredAt))|new Date(b.measuredAt).toLocaleString()|' "$PAGE" > "$own"
  sed '\|/vendor/chorus-time/boston.js|d' "$PAGE" > "$noscript"
  run boston_time "$own"; [ "$status" -eq 1 ] && [[ "$output" == *"bostonMinute"* ]] || { echo "own: $output"; return 1; }
  run boston_time "$noscript"; [ "$status" -eq 1 ] && [[ "$output" == *"boston.js"* ]] || { echo "noscript: $output"; return 1; }
}

@test "chorus-api serves the route and the page from this tree" {
  grep -q "app.get('/api/chorus/policies/counts'" "$ROOT/platform/api/src/server.ts" || return 1
  grep -q "fetchPolicyCounts" "$ROOT/platform/api/src/server.ts" || return 1
}

@test "the handler's behaviour: worst first, narrowed marked, door rows only, 503 on a dead door" {
  cd "$ROOT/platform/api"
  # the handler alone, without the package's global setup (it loads better-sqlite3, whose ABI
  # breaks under a role's system Node; Borg Queen 16:47)
  run npx jest --config '{"preset":"ts-jest","testEnvironment":"node"}' tests/policy-counts-4963.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  [[ "$output" == *"Tests:       10 passed, 10 total"* ]] || { echo "$output" | tail -5; return 1; }
}
