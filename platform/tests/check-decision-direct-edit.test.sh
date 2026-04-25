#!/usr/bin/env bash
# Hermetic tests for check-decision-direct-edit.sh (#2485 Move 5).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECK="${SCRIPT_DIR}/../scripts/check-decision-direct-edit.sh"

[ -x "$CHECK" ] || { echo "FAIL: $CHECK not executable" >&2; exit 1; }

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

cd "$FIXTURE"
git init -q
git config user.email t@t
git config user.name t

mkdir -p roles/silas/ontology
cat > roles/silas/ontology/chorus.ttl <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

chorus:Decision a rdfs:Class .
EOF
git add . && git commit -q -m initial

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

# Run 1: schema-only edit (no instance triple) — pass
cat >> roles/silas/ontology/chorus.ttl <<'EOF'

chorus:ADR a rdfs:Class .
EOF
git add roles/silas/ontology/chorus.ttl
out=$(echo "roles/silas/ontology/chorus.ttl" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "schema-only exit 0" 0 "$rc"
check "schema-only no violation" 0 "$(echo "$out" | grep -c 'decision-direct-edit')"
git checkout -q -- . && git reset -q HEAD

# Run 2: stages new chorus:Decision instance — fail
cat >> roles/silas/ontology/chorus.ttl <<'EOF'

chorus:dec-9999 a chorus:Decision ;
  rdfs:label "Rogue decision" .
EOF
git add roles/silas/ontology/chorus.ttl
out=$(echo "roles/silas/ontology/chorus.ttl" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "decision-instance exit 1" 1 "$rc"
check "decision-instance reports violation" 1 "$(echo "$out" | grep -c 'stages new chorus:Decision')"
git checkout -q -- . && git reset -q HEAD

# Run 3: stages new chorus:ADR instance — fail
cat >> roles/silas/ontology/chorus.ttl <<'EOF'

chorus:adr-099 a chorus:ADR ;
  rdfs:label "Rogue ADR" .
EOF
git add roles/silas/ontology/chorus.ttl
out=$(echo "roles/silas/ontology/chorus.ttl" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "adr-instance exit 1" 1 "$rc"
check "adr-instance reports violation" 1 "$(echo "$out" | grep -c 'stages new chorus:ADR')"

# Run 4: bypass via env var
out=$(echo "roles/silas/ontology/chorus.ttl" | DECISION_DIRECT_EDIT_SKIP=1 REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "skip-flag exit 0" 0 "$rc"
check "skip-flag bypass message" 1 "$(echo "$out" | grep -c 'bypassed')"
git checkout -q -- . && git reset -q HEAD

# Run 5: edit to unrelated file — pass (script ignores non-watched paths)
mkdir -p other
echo "x a chorus:Decision ." > other/junk.ttl
git add other/junk.ttl
out=$(echo "other/junk.ttl" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "non-watched-path exit 0" 0 "$rc"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
