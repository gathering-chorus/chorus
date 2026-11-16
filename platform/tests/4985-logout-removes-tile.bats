#!/usr/bin/env bats
# @test-type: unit — pulse's actor state and Clearing v2's tile filter, each on fixtures
# @domain: roles
# @card: #4985
#
# AC4: logging a role out removes its tile within one refresh. Logout closes the pane session
# (endedAt); pulse reads the role 'off'; Clearing v2's people route (re-read every few seconds,
# #4646) drops an 'off' role (views.ts isRoleLoggedIn). A crash ends on a fact too (Wren 16:53):
# the sweep ends the run when its pid is gone, and a role with no live run reads 'off'.
#
# Covers: platform/pulse/src/actor-state.ts
# Covers: directing/clearing-v2/src/views.ts

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  PULSE="$ROOT/platform/pulse"
  V2="$ROOT/directing/clearing-v2"
}

@test "a logged-out pane, or one whose run crashed, reads off in pulse" {
  run bash -c "cd '$PULSE' && node_modules/.bin/jest --rootDir . src/actor-state-4985.test.ts -t 'logged out|run crashed' 2>&1"
  echo "$output" | tail -5
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE '2 passed'
}

@test "an off role has no tile in Clearing v2's people row" {
  run bash -c "cd '$V2' && node_modules/.bin/jest tests/rooms-ux-4639.test.ts 2>&1"
  echo "$output" | tail -5
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a pulse that ignores logout on a pane keeps the role on" {
  work="$BATS_TEST_TMPDIR/pulse"
  mkdir -p "$work"
  cp -R "$PULSE/src" "$PULSE/jest.config.js" "$PULSE/tsconfig.json" "$PULSE/test-env.setup.js" "$work/"
  ln -s "$PULSE/node_modules" "$work/node_modules"
  sed -i '' "s/const isClosed = Boolean(s.endedAt) || s.sessionState === 'closed';/const isClosed = false;/" "$work/src/actor-state.ts"
  if cmp -s "$PULSE/src/actor-state.ts" "$work/src/actor-state.ts"; then echo "mutation did not apply"; return 1; fi
  run bash -c "cd '$work' && node_modules/.bin/jest --rootDir . src/actor-state-4985.test.ts -t 'logged out' 2>&1"
  [ "$status" -ne 0 ]
}
