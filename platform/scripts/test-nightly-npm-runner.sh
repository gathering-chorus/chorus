#!/usr/bin/env bash
# test-nightly-npm-runner.sh — #3606: the nightly npm runner must honor a
# package's OWN test runner instead of hardcoding `npx jest`.
#
# The mcp-server red: its scripts.test is `tsx --test` (node test runner, no
# jest anywhere), but run_one_attempt ran `npx jest`, which tried to download
# jest into the shared npx cache at 3am and died (ENOTEMPTY) with EMPTY output
# — the blank-summary fail that sat red for weeks while the package's real
# suite (104 tests) passed.
#
# Contract under test (via the --run-one CLI added by #3606):
#   - non-jest package, passing tests  → SUITE|npm|...|pass| + real summary
#   - non-jest package, failing tests  → SUITE|npm|...|fail|
#   - no `npx jest` invocation happens for non-jest packages (no cache download)
#
# Hermetic: fixtures are tmp dirs with stub package.json; no live services,
# no shared caches, no writes outside mktemp.

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"

PASS=0; FAIL=0
p() { PASS=$((PASS+1)); echo "  ok: $1"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $1"; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# --- fixture 1: non-jest package whose tests pass (node test runner shape) ---
mkdir -p "$TMP/nodepass"
cat > "$TMP/nodepass/package.json" <<'EOF'
{
  "name": "fixture-nodepass",
  "scripts": { "test": "node -e \"console.log('# tests 3'); console.log('# pass 3'); console.log('# fail 0')\"" }
}
EOF

# --- fixture 2: non-jest package whose tests fail ---
mkdir -p "$TMP/nodefail"
cat > "$TMP/nodefail/package.json" <<'EOF'
{
  "name": "fixture-nodefail",
  "scripts": { "test": "node -e \"console.log('# tests 2'); console.log('# pass 1'); console.log('# fail 1'); process.exit(1)\"" }
}
EOF

echo "--- non-jest pass package ---"
OUT=$(bash "$NIGHTLY" --run-one npm "$TMP/nodepass" 2>&1)
LINE=$(echo "$OUT" | grep "^SUITE|npm|" | head -1)
echo "$LINE" | grep -q "|pass|" && p "non-jest passing package graded pass" || f "expected |pass|, got: $LINE"
echo "$LINE" | grep -q "pass 3" && p "summary carries the runner's own counts" || f "summary lost the counts: $LINE"

echo "--- non-jest fail package ---"
OUT=$(bash "$NIGHTLY" --run-one npm "$TMP/nodefail" 2>&1)
LINE=$(echo "$OUT" | grep "^SUITE|npm|" | head -1)
echo "$LINE" | grep -q "|fail|" && p "non-jest failing package graded fail" || f "expected |fail|, got: $LINE"

echo "--- no jest download attempted for non-jest package ---"
# `npx jest` against the fixture would fail loudly with npm-cache noise or a
# jest install attempt; the runner must never mention jest for these packages.
OUT=$(bash "$NIGHTLY" --run-one npm "$TMP/nodepass" 2>&1)
if echo "$OUT" | grep -qi "jest"; then f "runner invoked jest for a non-jest package: $(echo "$OUT" | grep -i jest | head -1)"; else p "no jest invocation for non-jest package"; fi

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
