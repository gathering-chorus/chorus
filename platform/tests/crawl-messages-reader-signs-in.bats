#!/usr/bin/env bats
# @test-type: unit — pulse's Buzz reader against a stub relay that checks the sign-in, and
# chorus-crawl's reader environment; no live relay
# @domain: messages
# @card: #4984
#
# AC1: the crawler's messages reader signs in with the relay's public address
# (BUZZ_AUTH_RELAY_URL). Since the relay's 10-25 cutover a sign-in naming ws://<host> is
# refused; the sensor was unmeasured every day from 10-26 to 11-16.
#
# Covers: platform/pulse/src/buzz-day-counts.ts
# Covers: platform/services/chorus-crawl/src/main.rs
# Covers: platform/services/chorus-crawl/src/readings.rs

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  PULSE="$ROOT/platform/pulse"
  CRAWL="$ROOT/platform/services/chorus-crawl/src"
}

jest_in() {  # $1 = a pulse tree, $2 = test name filter
  ( cd "$1" && "$PULSE/node_modules/.bin/jest" --rootDir . src/buzz-day-counts-4984.test.ts -t "$2" 2>&1 )
}

@test "the reader's sign-in names the public address when BUZZ_AUTH_RELAY_URL is given" {
  run jest_in "$PULSE" 'AC1'
  echo "$output" | tail -4
  [ "$status" -eq 0 ]
}

@test "chorus-crawl runs the reader with BUZZ_AUTH_RELAY_URL, and is unmeasured without it (never a guess)" {
  grep -q '\.envs(reader_env(&relay, &relay_host, &auth_url))' "$CRAWL/main.rs"
  grep -q '("BUZZ_AUTH_RELAY_URL", auth_relay_url.to_string())' "$CRAWL/readings.rs"
  grep -q "the relay's public address is unknown (BUZZ_AUTH_RELAY_URL unset" "$CRAWL/main.rs"
}

@test "NEGATIVE PROOF: a reader that signs in without the public address is refused, and the check fails" {
  work="$BATS_TEST_TMPDIR/pulse"; mkdir -p "$work"
  cp -R "$PULSE/src" "$PULSE/jest.config.js" "$PULSE/tsconfig.json" "$PULSE/test-env.setup.js" "$work/"
  ln -s "$PULSE/node_modules" "$work/node_modules"
  sed -i '' "s/  const stated = (env.BUZZ_AUTH_RELAY_URL || '').trim();/  const stated = '';/" "$work/src/buzz-writer.ts"
  if cmp -s "$PULSE/src/buzz-writer.ts" "$work/src/buzz-writer.ts"; then echo "mutation did not apply"; return 1; fi
  run jest_in "$work" 'AC1'
  [ "$status" -ne 0 ]
}
