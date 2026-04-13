#!/usr/bin/env bats
# gate-code-execsync.bats — Tests for execSync lint gate (#2000)
# What Jeff sees: app hangs in production because execSync blocks the event loop.
# This gate catches it before demo, not after deploy.

LINT_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/gate-code-lint.sh"
APP_ROOT="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"
FIXTURES="/tmp/gate-code-lint-fixtures"

setup() {
  mkdir -p "$FIXTURES/src/handlers" "$FIXTURES/src/services" "$FIXTURES/src/middleware"
  mkdir -p "$FIXTURES/scripts" "$FIXTURES/tests"
}

teardown() {
  rm -rf "$FIXTURES"
}

# --- AC 1: gate:code lint checks changed .ts files for execSync usage ---

@test "lint script exists and is executable" {
  [ -f "$LINT_SCRIPT" ]
  [ -x "$LINT_SCRIPT" ]
}

# --- AC 2: FAIL if execSync appears in request-path code ---

@test "FAIL when execSync in src/handlers/" {
  echo 'const result = execSync("ls");' > "$FIXTURES/src/handlers/test.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/handlers/test.ts"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "execSync" ]]
}

@test "FAIL when execSync in src/services/" {
  echo 'import { execSync } from "child_process";' > "$FIXTURES/src/services/monitor.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/services/monitor.ts"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "execSync" ]]
}

@test "FAIL when execSync in src/middleware/" {
  echo 'execSync("whoami");' > "$FIXTURES/src/middleware/auth.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/middleware/auth.ts"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "execSync" ]]
}

# --- AC 3: PASS if execSync is in scripts/, tests/, or build tooling ---

@test "PASS when execSync in scripts/" {
  echo 'execSync("npm run build");' > "$FIXTURES/scripts/deploy.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/scripts/deploy.ts"
  [ "$status" -eq 0 ]
}

@test "PASS when execSync in tests/" {
  echo 'execSync("curl localhost");' > "$FIXTURES/tests/smoke.test.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/tests/smoke.test.ts"
  [ "$status" -eq 0 ]
}

@test "PASS when no execSync in request-path code" {
  echo 'const result = await exec("ls");' > "$FIXTURES/src/handlers/clean.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/handlers/clean.ts"
  [ "$status" -eq 0 ]
}

# --- Edge cases ---

@test "FAIL when execSync in a comment is still flagged" {
  # Conservative: even commented-out execSync should flag, since it might get uncommented
  echo '// execSync("dangerous");' > "$FIXTURES/src/handlers/commented.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/handlers/commented.ts"
  [ "$status" -ne 0 ]
}

@test "handles multiple files — one bad file fails the whole check" {
  echo 'const x = 1;' > "$FIXTURES/src/handlers/clean.ts"
  echo 'execSync("bad");' > "$FIXTURES/src/services/bad.ts"
  run bash "$LINT_SCRIPT" "$FIXTURES/src/handlers/clean.ts" "$FIXTURES/src/services/bad.ts"
  [ "$status" -ne 0 ]
}
