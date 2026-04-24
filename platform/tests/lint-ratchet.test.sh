#!/usr/bin/env bash
# #2462: hermetic tests for lint-ratchet.js.
# Creates a fixture repo with one .ts file, runs ratchet against it with controlled baselines.

set -uo pipefail

SCRIPT="$(cd "$(dirname "$0")/../scripts" && pwd)/lint-ratchet.js"
PASSED=0
FAILED=0

assert() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASSED=$((PASSED + 1))
  else
    echo "  FAIL: $label — expected='$expected' actual='$actual'"
    FAILED=$((FAILED + 1))
  fi
}

# Set up a fixture repo.
FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/src"
# Symlink chorus node_modules so the fixture can require '@typescript-eslint/*' by name.
ln -s "$(cd "$(dirname "$0")/../.." && pwd)/node_modules" "$FIXTURE/node_modules"

cat > "$FIXTURE/src/a.ts" <<'EOF'
// Two no-explicit-any, one semi.
const x: any = 1;
const y: any = 2;
console.log(x, y)
EOF

cat > "$FIXTURE/eslint.config.js" <<'EOF'
const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');
module.exports = [
  {
    files: ['**/*.ts'],
    languageOptions: { parser: tsParser },
    plugins: { '@typescript-eslint': tsPlugin },
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      'semi': ['error', 'always'],
    },
  },
];
EOF

export LINT_RATCHET_ROOT="$FIXTURE"
export LINT_RATCHET_BASELINE="$FIXTURE/.baseline.json"
export LINT_RATCHET_GLOBS="src/**/*.ts"

# Test 1: --regenerate creates baseline.
node "$SCRIPT" --regenerate >/dev/null 2>&1
rc=$?
assert "regenerate exit 0" "0" "$rc"
assert "baseline file exists" "y" "$([ -f "$FIXTURE/.baseline.json" ] && echo y || echo n)"

EXPLICIT_ANY=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$FIXTURE/.baseline.json','utf8')).counts['@typescript-eslint/no-explicit-any']||0)")
assert "baseline captures no-explicit-any=2" "2" "$EXPLICIT_ANY"

# Test 2: check passes at baseline.
node "$SCRIPT" >/dev/null 2>&1
rc=$?
assert "check exit 0 at baseline" "0" "$rc"

# Test 3: violation fails with exit 1.
cat > "$FIXTURE/src/b.ts" <<'EOF'
const z: any = 3;
const w: any = 4;
EOF
node "$SCRIPT" >/dev/null 2>&1
rc=$?
assert "violation exits 1" "1" "$rc"

# Test 4: drop triggers advisory (still exit 0).
rm "$FIXTURE/src/b.ts"
cat > "$FIXTURE/src/a.ts" <<'EOF'
const x: number = 1;
const y: number = 2;
console.log(x, y);
EOF
OUTPUT=$(node "$SCRIPT" 2>&1)
rc=$?
assert "drop exits 0" "0" "$rc"
if echo "$OUTPUT" | grep -q 'Drops since baseline'; then
  echo "  PASS: drop reports advisory"
  PASSED=$((PASSED + 1))
else
  echo "  FAIL: drop should report advisory — got: $OUTPUT"
  FAILED=$((FAILED + 1))
fi

# Test 5: new rule firing that isn't in baseline -> exit 2.
# Remove semi from our fixture's baseline (simulate a rule-enable without regen).
node -e "
const fs = require('fs');
const p = '$FIXTURE/.baseline.json';
const d = JSON.parse(fs.readFileSync(p, 'utf8'));
delete d.counts.semi;
fs.writeFileSync(p, JSON.stringify(d, null, 2));
"
cat > "$FIXTURE/src/a.ts" <<'EOF'
const x = 1
EOF
node "$SCRIPT" >/dev/null 2>&1
rc=$?
assert "new-rule exits 2" "2" "$rc"

echo ""
echo "Result: $PASSED passed, $FAILED failed"
[ "$FAILED" = "0" ]
