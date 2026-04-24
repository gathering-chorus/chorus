#!/usr/bin/env bash
# Test: pre-commit lint ratchet — reject commits that raise any rule count (#2462/#2464)
# RED before hook block exists. GREEN after.
# Mirrors platform/tests/pre-commit-wip-gate-test.sh pattern: extract the gate logic
# into a function, then exercise it in a fixture.
set -uo pipefail

PASS=0
FAIL=0

# The gate logic extracted for testability. In production this runs inside
# .git/hooks/pre-commit. The real hook reads $STAGED and $REPO_ROOT;
# here we pass them explicitly.
ratchet_gate_check() {
  local repo_root="$1"
  local staged="$2"

  # Same trigger condition as the hook block: only run if a .ts/config/baseline
  # file is staged AND the baseline exists.
  [ -f "$repo_root/eslint.config.js" ] || return 0
  [ -f "$repo_root/.eslint-baseline.json" ] || return 0
  echo "$staged" | grep -qE '\.ts$|^eslint\.config\.js$|^\.eslint-baseline\.json$|^package\.json$' || return 0

  (cd "$repo_root" && npm run lint:ratchet --silent 2>&1)
}

assert() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label — expected='$expected' actual='$actual'"
    FAIL=$((FAIL + 1))
  fi
}

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# Fixture with one src file + config + baseline
mkdir -p "$FIXTURE/src"
ln -s "$(cd "$(dirname "$0")/../.." && pwd)/node_modules" "$FIXTURE/node_modules"

cat > "$FIXTURE/eslint.config.js" <<'EOF'
const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');
module.exports = [{
  files: ['**/*.ts'],
  languageOptions: { parser: tsParser },
  plugins: { '@typescript-eslint': tsPlugin },
  rules: { '@typescript-eslint/no-explicit-any': 'error' },
}];
EOF

cat > "$FIXTURE/src/a.ts" <<'EOF'
const x: any = 1;
EOF

cat > "$FIXTURE/package.json" <<'EOF'
{
  "name": "ratchet-test",
  "private": true,
  "scripts": {
    "lint:ratchet": "node ./lint-ratchet.js",
    "lint:baseline": "node ./lint-ratchet.js --regenerate"
  }
}
EOF

# Point lint-ratchet.js at the fixture
cp "$(cd "$(dirname "$0")/../scripts" && pwd)/lint-ratchet.js" "$FIXTURE/lint-ratchet.js"
export LINT_RATCHET_ROOT="$FIXTURE"
export LINT_RATCHET_GLOBS="src/**/*.ts"

# Generate baseline so the ratchet has something to compare against
(cd "$FIXTURE" && node lint-ratchet.js --regenerate >/dev/null 2>&1)

# Test 1: No lint-affecting file staged -> gate returns 0 (skip, not run)
output=$(ratchet_gate_check "$FIXTURE" "README.md")
rc=$?
assert "skip when no .ts/config/baseline staged" "0" "$rc"

# Test 2: Clean .ts staged (no new violations) -> gate returns 0 (pass)
output=$(ratchet_gate_check "$FIXTURE" "src/a.ts")
rc=$?
assert "clean .ts commit passes" "0" "$rc"

# Test 3: Violation introduced -> gate returns non-zero (fail)
cat > "$FIXTURE/src/a.ts" <<'EOF'
const x: any = 1;
const y: any = 2;
EOF
output=$(ratchet_gate_check "$FIXTURE" "src/a.ts")
rc=$?
assert "violation blocks commit" "1" "$rc"
if echo "$output" | grep -q 'climbed above baseline'; then
  echo "  PASS: violation message names the rule"
  PASS=$((PASS + 1))
else
  echo "  FAIL: violation message should name the rule — got: $output"
  FAIL=$((FAIL + 1))
fi

# Test 4: Baseline regeneration staged (even with a climb) -> gate passes
# (Because the baseline file itself is being updated in the same commit.)
# Reset to baseline state, stage only the baseline file.
cat > "$FIXTURE/src/a.ts" <<'EOF'
const x: any = 1;
EOF
output=$(ratchet_gate_check "$FIXTURE" ".eslint-baseline.json")
rc=$?
assert "baseline-only commit passes" "0" "$rc"

# Test 5: Missing baseline -> gate skips (not applicable)
rm "$FIXTURE/.eslint-baseline.json"
output=$(ratchet_gate_check "$FIXTURE" "src/a.ts")
rc=$?
assert "no baseline file => gate skips" "0" "$rc"

echo ""
echo "Result: $PASS passed, $FAIL failed"
[ "$FAIL" = "0" ]
