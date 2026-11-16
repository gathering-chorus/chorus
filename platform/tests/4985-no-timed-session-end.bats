#!/usr/bin/env bats
# @test-type: unit — runs pulse's #4985 actor-state tests, then the same tests on a mutated copy
# @domain: roles
# @card: #4985
#
# AC3: no code path ends a role session on elapsed time alone. Pulse reads a pane's liveness
# (actor-state.ts deriveSessions); the sweep ends only crashed runs (pid gone) and superseded
# sessions. A pane quiet five hours stays live; putting the quiet rule back on panes is red.
#
# Covers: platform/pulse/src/actor-state.ts
# Covers: platform/pulse/src/actor-state-4985.test.ts

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  PULSE="$ROOT/platform/pulse"
}

jest_in() {  # $1 = a pulse tree, $2 = test name filter
  ( cd "$1" && "$PULSE/node_modules/.bin/jest" --rootDir . src/actor-state-4985.test.ts -t "$2" 2>&1 )
}

copy_pulse() {
  work="$BATS_TEST_TMPDIR/pulse"
  mkdir -p "$work"
  cp -R "$PULSE/src" "$PULSE/jest.config.js" "$PULSE/tsconfig.json" "$PULSE/test-env.setup.js" "$work/"
  ln -s "$PULSE/node_modules" "$work/node_modules"
}

@test "a quiet logged-in pane stays live; only logout or a crashed run ends it" {
  run jest_in "$PULSE" '#4985'
  echo "$output" | tail -5
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a pulse that ends a quiet pane after 30 minutes fails" {
  copy_pulse
  sed -i '' "s/const isStale = s.channel !== 'pane' \&\& (/const isStale = (/" "$work/src/actor-state.ts"
  if cmp -s "$PULSE/src/actor-state.ts" "$work/src/actor-state.ts"; then echo "mutation did not apply"; return 1; fi
  run jest_in "$work" 'quiet for five hours'
  [ "$status" -ne 0 ]
}
