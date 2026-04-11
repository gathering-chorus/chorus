#!/usr/bin/env bash
# Test: SPARQL prefix lint — #1909 AC item 1
# Verifies that sparql-prefix-lint.sh catches undeclared prefixes and passes clean files.
set -euo pipefail

LINT_SCRIPT="$(dirname "$0")/../scripts/sparql-prefix-lint.sh"
TMPDIR=$(mktemp -d)
PASS=0
FAIL=0

cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

run_test() {
  local name="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

run_test_expect_fail() {
  local name="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "  FAIL: $name (expected failure but got success)"
    FAIL=$((FAIL + 1))
  else
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  fi
}

echo "=== SPARQL prefix lint tests ==="

# 1. Script exists and is executable
run_test "lint script exists" test -x "$LINT_SCRIPT"

# 2. Clean file passes — all used prefixes declared
cat > "$TMPDIR/clean.sparql" <<'SPARQL'
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?label WHERE {
  GRAPH <urn:chorus:ontology> {
    chorus:loom-principles rdfs:label ?label .
  }
}
SPARQL
run_test "clean file passes" bash "$LINT_SCRIPT" "$TMPDIR"

# 3. Missing prefix fails — uses owl: without declaring it
cat > "$TMPDIR/broken.sparql" <<'SPARQL'
PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?type WHERE {
  GRAPH <urn:chorus:ontology> {
    ?x a owl:NamedIndividual .
  }
}
SPARQL
run_test_expect_fail "undeclared owl: prefix fails" bash "$LINT_SCRIPT" "$TMPDIR"

# 4. File with no prefixes and no prefix usage passes (like health.sparql)
rm "$TMPDIR"/*.sparql
cat > "$TMPDIR/noprefix.sparql" <<'SPARQL'
SELECT (COUNT(*) AS ?count) WHERE { GRAPH <urn:chorus:ontology> { ?s ?p ?o } }
SPARQL
run_test "no-prefix file passes" bash "$LINT_SCRIPT" "$TMPDIR"

# 5. Real sparql dir passes (current src/sparql/ should be clean after #1901 fix)
run_test "real sparql dir passes" bash "$LINT_SCRIPT" "$(dirname "$0")/../api/src/sparql"

# 6. dist/sparql sync — src and dist should match
SRC_DIR="$(dirname "$0")/../api/src/sparql"
DIST_DIR="$(dirname "$0")/../api/dist/sparql"
if [ -d "$DIST_DIR" ]; then
  SYNC_OK=true
  for src_file in "$SRC_DIR"/*.sparql; do
    bn=$(basename "$src_file")
    dist_file="$DIST_DIR/$bn"
    if [ -f "$dist_file" ]; then
      if ! diff -q "$src_file" "$dist_file" >/dev/null 2>&1; then
        SYNC_OK=false
      fi
    else
      SYNC_OK=false
    fi
  done
  if $SYNC_OK; then
    echo "  PASS: dist/sparql in sync with src/sparql"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: dist/sparql out of sync with src/sparql"
    FAIL=$((FAIL + 1))
  fi
else
  echo "  SKIP: dist/sparql does not exist"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
