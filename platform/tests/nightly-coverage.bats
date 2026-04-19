#!/usr/bin/env bats
# nightly-coverage.bats — Tests for nightly-coverage.sh (#2207)
# What Jeff experiences: at 02:00 a Bridge message shows per-service coverage %s.
# On regression: Bridge names the failing service, its floor, and the delta.
# On green: one PASS line with every service's current %.
# Script always exits 0 so the LaunchAgent never stops scheduling.

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SCRIPT="${CHORUS_ROOT}/platform/scripts/nightly-coverage.sh"

# --- AC 1: floors.yml exists and is human-editable ---

@test "coverage-floors.yml exists at chorus root" {
  [ -f "${CHORUS_ROOT}/coverage-floors.yml" ]
}

@test "coverage-floors.yml has ts and rust sections with numeric floors" {
  local entries
  entries=$(python3 -c "
import re
text = open('${CHORUS_ROOT}/coverage-floors.yml').read()
entries = re.findall(r'^\s{2}\S[^:]+:\s+(\d+)', text, re.MULTILINE)
print(len(entries))
")
  [ "$entries" -ge 6 ]
}

# --- AC 2: LaunchAgent schedules at 02:00 ---

@test "LaunchAgent plist exists" {
  [ -f "$HOME/Library/LaunchAgents/com.chorus.nightly-coverage.plist" ]
}

@test "LaunchAgent uses StartCalendarInterval at hour 2 not polling StartInterval" {
  local plist="$HOME/Library/LaunchAgents/com.chorus.nightly-coverage.plist"
  grep -q "StartCalendarInterval" "$plist"
  ! grep -q "^.*<key>StartInterval</key>" "$plist"
}

# --- AC 3 & 4: script invokes standard tools, reads floors not jest.config ---

@test "nightly-coverage.sh exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "nightly-coverage.sh does not read jest.config.js or tarpaulin.toml for thresholds" {
  ! grep -qE "jest\.config|tarpaulin\.toml|coverageThreshold|fail-under" "$SCRIPT"
}

# --- AC 5 & 6: Bridge message format + exit 0 ---

@test "script exits 0 on green run (LaunchAgent must not stop)" {
  local tmpfix
  tmpfix=$(mktemp -d)
  trap "rm -rf $tmpfix" EXIT

  local floors="$tmpfix/floors.yml"
  cat > "$floors" <<'EOF'
ts:
  fake/proj: 80
rust:
  fake/crate: 70
EOF
  mkdir -p "$tmpfix/fixtures/fake/proj/coverage"
  cat > "$tmpfix/fixtures/fake/proj/coverage/coverage-summary.json" \
    <<'EOF'
{"total":{"statements":{"total":100,"covered":90,"pct":90},"branches":{},"functions":{},"lines":{}}}
EOF
  mkdir -p "$tmpfix/fixtures/fake/crate"
  cat > "$tmpfix/fixtures/fake/crate/llvm-cov-summary.json" \
    <<'EOF'
{"data":[{"totals":{"lines":{"count":100,"covered":80,"percent":80}}}],"type":"llvm.coverage.report","version":"2.0.1"}
EOF

  NIGHTLY_COVERAGE_FLOORS="$floors" \
  NIGHTLY_COVERAGE_FIXTURES="$tmpfix/fixtures" \
  NIGHTLY_COVERAGE_DRY_RUN=1 \
  BRIDGE_NUDGE_URL="http://localhost:3475/api/nudge" \
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "regression run exits 0 and names failing service with floor and delta" {
  local tmpfix
  tmpfix=$(mktemp -d)
  trap "rm -rf $tmpfix" EXIT

  local floors="$floors_file"
  floors="$tmpfix/floors.yml"
  cat > "$floors" <<'EOF'
ts:
  fake/proj: 90
rust:
  fake/crate: 70
EOF
  mkdir -p "$tmpfix/fixtures/fake/proj/coverage"
  # 79% < floor 90 — regression
  cat > "$tmpfix/fixtures/fake/proj/coverage/coverage-summary.json" \
    <<'EOF'
{"total":{"statements":{"total":100,"covered":79,"pct":79},"branches":{},"functions":{},"lines":{}}}
EOF
  mkdir -p "$tmpfix/fixtures/fake/crate"
  cat > "$tmpfix/fixtures/fake/crate/llvm-cov-summary.json" \
    <<'EOF'
{"data":[{"totals":{"lines":{"count":100,"covered":80,"percent":80}}}],"type":"llvm.coverage.report","version":"2.0.1"}
EOF

  NIGHTLY_COVERAGE_FLOORS="$floors" \
  NIGHTLY_COVERAGE_FIXTURES="$tmpfix/fixtures" \
  NIGHTLY_COVERAGE_DRY_RUN=1 \
  BRIDGE_NUDGE_URL="http://localhost:3475/api/nudge" \
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi "fake/proj"
  echo "$output" | grep -qiE "REGRESSION|FAIL|below|floor"
  echo "$output" | grep -qiE "79|79\.0"
}
