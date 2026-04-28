#!/usr/bin/env bash
# #2532 AC5 — verification: prove `jest --randomize` surfaces order-dependent
# tests. Builds a hermetic 2-test fixture where test B depends on a module
# global mutated by test A. Without randomization the file order (A then B)
# always passes; with --randomize, B-first runs fail. Asserts at least one
# failure across N attempts (P(all-pass) = 1/2^N for two tests).
#
# Mirrors the clippy-ratchet.test.sh pattern: hermetic, no network, fast.

set -uo pipefail

JEST_BIN="$(cd "$(dirname "$0")/.." && pwd)/api/node_modules/.bin/jest"
ATTEMPTS="${ATTEMPTS:-10}"
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

if [ ! -x "$JEST_BIN" ]; then
  echo "FAIL: jest binary not found at $JEST_BIN"
  echo "      run npm ci in platform/api first"
  exit 2
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

cat > "$FIXTURE/order-dependent.test.js" <<'EOF'
// Two tests sharing module-level state. Test A initializes; test B asserts.
// File-order (A then B) passes; reverse order fails. --randomize shuffles
// within-file order, so over enough attempts B-first must occur.
let initialized = false;

test('A: initializes shared state', () => {
  initialized = true;
  expect(initialized).toBe(true);
});

test('B: requires shared state from A', () => {
  expect(initialized).toBe(true);
});
EOF

# Probe: confirm --randomize is recognized on this jest.
if ! "$JEST_BIN" --help 2>&1 | grep -q -- '--randomize'; then
  echo "FAIL: jest at $JEST_BIN does not support --randomize"
  exit 2
fi

echo "Running up to $ATTEMPTS shuffled attempts against order-dependent fixture..."
SAW_FAILURE=0
for i in $(seq 1 "$ATTEMPTS"); do
  if ! "$JEST_BIN" --rootDir "$FIXTURE" --randomize --silent >/dev/null 2>&1; then
    SAW_FAILURE=1
    echo "  attempt $i: FAIL (expected — randomize caught the order dep)"
    break
  fi
done

if [ "$SAW_FAILURE" = "1" ]; then
  assert "randomize-detects-order-dependence" "yes" "yes"
else
  assert "randomize-detects-order-dependence" "yes" "no — $ATTEMPTS attempts all passed"
fi

# Control: without --randomize, file order is deterministic A->B and must
# always pass. Confirms the failure above came from shuffling, not breakage.
if "$JEST_BIN" --rootDir "$FIXTURE" --silent >/dev/null 2>&1; then
  assert "control-no-randomize-passes" "yes" "yes"
else
  assert "control-no-randomize-passes" "yes" "no — fixture broken even in file order"
fi

echo
echo "Results: $PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
