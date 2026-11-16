#!/usr/bin/env bats
# @test-type: unit — pulse's Buzz reader against a stub relay that refuses the sign-in
# @domain: messages
# @card: #4984
#
# AC2: a refused relay sign-in is an error at once, naming the refusal, never waited out.
# Before #4984 the reader dropped a CLOSED that was not rate-limited and waited 60 s for an
# EOSE that never came ("relay sent no EOSE within 60000ms (page 2)", daily since 10-26).
#
# Covers: platform/pulse/src/buzz-day-counts.ts

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  PULSE="$ROOT/platform/pulse"
}

jest_in() {  # $1 = a pulse tree, $2 = test name filter
  ( cd "$1" && "$PULSE/node_modules/.bin/jest" --rootDir . src/buzz-day-counts-4984.test.ts -t "$2" 2>&1 )
}

@test "a refused sign-in is an error within seconds, naming the relay's refusal" {
  run jest_in "$PULSE" 'AC2'
  echo "$output" | tail -4
  [ "$status" -eq 0 ]
}

@test "the control: a relay that accepts the sign-in reports no error" {
  run jest_in "$PULSE" 'AC1'
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a reader that ignores the refusal fails the check" {
  work="$BATS_TEST_TMPDIR/pulse"; mkdir -p "$work"
  cp -R "$PULSE/src" "$PULSE/jest.config.js" "$PULSE/tsconfig.json" "$PULSE/test-env.setup.js" "$work/"
  ln -s "$PULSE/node_modules" "$work/node_modules"
  sed -i '' "s/else if (f\[0\] === 'CLOSED' \&\& ours \&\& page > authPage) {/else if (false) {/" "$work/src/buzz-day-counts.ts"
  if cmp -s "$PULSE/src/buzz-day-counts.ts" "$work/src/buzz-day-counts.ts"; then echo "mutation did not apply"; return 1; fi
  run jest_in "$work" 'AC2'
  [ "$status" -ne 0 ]
}
