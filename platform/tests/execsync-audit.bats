#!/usr/bin/env bats
# @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
load test_helper
# execsync-audit.bats — Tests for execSync audit (#1999)
# What Jeff sees: app hangs for 797s because execSync blocks the event loop.
# After the fix: zero execSync on request paths.

APP_SRC="${HOME}/CascadeProjects/jeff-bridwell-personal-site/src"
LINT="${CHORUS_ROOT}/platform/scripts/gate-code-lint.sh"

# --- AC 1: Audit exists (this test IS the audit) ---

@test "MonitoringService.ts has no execSync" {
  ! grep -q 'execSync' "$APP_SRC/services/MonitoringService.ts"
}

@test "icd.service.ts has no execSync on request path" {
  ! grep -q 'execSync' "$APP_SRC/services/icd.service.ts"
}

@test "git-churn.service.ts has no execSync on request path" {
  ! grep -q 'execSync' "$APP_SRC/services/git-churn.service.ts"
}

# --- AC 2+3: Module-load cached versions are acceptable ---

@test "correlation.middleware.ts execSync is module-load cached only" {
  # Allowed: execSync at module load, cached in variable, runs once at startup
  count=$(grep -c 'execSync' "$APP_SRC/middleware/correlation.middleware.ts")
  # Should be exactly 2 (import + one cached call)
  [ "$count" -le 2 ]
  # Must be cached pattern
  grep -q 'cachedVersion' "$APP_SRC/middleware/correlation.middleware.ts"
}

@test "spine-event.ts execSync is module-load cached only" {
  count=$(grep -c 'execSync' "$APP_SRC/utils/spine-event.ts")
  [ "$count" -le 2 ]
  grep -q 'cachedVersion' "$APP_SRC/utils/spine-event.ts"
}

# --- AC 4: Zero execSync on request paths ---

@test "gate-code-lint passes on all request-path services and handlers" {
  # correlation.middleware.ts has a module-load cached execSync (tested separately in test 4)
  run bash "$LINT" \
    "$APP_SRC/services/MonitoringService.ts" \
    "$APP_SRC/services/icd.service.ts" \
    "$APP_SRC/services/git-churn.service.ts" \
    "$APP_SRC/handlers/"*.ts
  [ "$status" -eq 0 ]
}
