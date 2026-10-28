#!/usr/bin/env bats
# @test-type: unit — #4765 AC3: bridge-subscriber accepts AGY roles (borg-queen, abby-normal)
# @card: 4765 · owner: borg-queen

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO/platform/scripts/bridge-subscriber.js"

@test "negative: unknown role is refused" {
  run node "$SCRIPT" invalid-role-xyz
  [ "$status" -eq 1 ]
  [[ "$output" == *"bad role 'invalid-role-xyz'"* ]] || return 1
}

@test "negative: empty role is refused" {
  run node "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"bad role ''"* ]] || return 1
}

@test "AC3: borg-queen is accepted as valid role" {
  run node -e "
    const cp = require('child_process');
    const proc = cp.spawn('node', ['$SCRIPT', 'borg-queen']);
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d; });
    setTimeout(() => {
      proc.kill('SIGKILL');
      if (stderr.includes('bad role')) process.exit(1);
      process.exit(0);
    }, 500);
  "
  [ "$status" -eq 0 ]
}

@test "AC3: abby-normal is accepted as valid role" {
  run node -e "
    const cp = require('child_process');
    const proc = cp.spawn('node', ['$SCRIPT', 'abby-normal']);
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d; });
    setTimeout(() => {
      proc.kill('SIGKILL');
      if (stderr.includes('bad role')) process.exit(1);
      process.exit(0);
    }, 500);
  "
  [ "$status" -eq 0 ]
}
