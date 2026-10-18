#!/usr/bin/env bats
# @test-type: unit — reads the repo's declared plist only
# @domain: search — placed by chorus-crawl's domain rules (#4538)
#
# #4605 reopen: Clearing v2 room messages reach search only when the scheduled
# indexer (com.chorus.reindex-worker) can read the Buzz relay, and the deps layer
# wires fetchBuzz only when BUZZ_RELAY_HOST is set. #4605 landed with the reader
# fixed but the worker's env missing the key, so prod indexed 0 room messages.

PLIST="${BATS_TEST_DIRNAME}/../../config/launchagents/com.chorus.reindex-worker.plist"

relay_host() {
  /usr/libexec/PlistBuddy -c "Print :EnvironmentVariables:BUZZ_RELAY_HOST" "$1" 2>/dev/null
}

@test "the reindex worker plist declares BUZZ_RELAY_HOST" {
  [ -f "$PLIST" ]
  run relay_host "$PLIST"
  [ "$status" -eq 0 ]
  [ -n "$output" ]
}

@test "negative proof: a worker plist without the key is caught" {
  tmp="$BATS_TEST_TMPDIR/worker.plist"
  cp "$PLIST" "$tmp"
  /usr/libexec/PlistBuddy -c "Delete :EnvironmentVariables:BUZZ_RELAY_HOST" "$tmp" 2>/dev/null || true
  run relay_host "$tmp"
  [ "$status" -ne 0 ] || [ -z "$output" ]
}

@test "the deps layer wires fetchBuzz only from BUZZ_RELAY_HOST (the key this plist must carry)" {
  grep -q "process.env.BUZZ_RELAY_HOST" "${BATS_TEST_DIRNAME}/../api/src/index-all-sources-deps.ts"
}
