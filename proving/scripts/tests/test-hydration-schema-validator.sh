#!/usr/bin/env bash
# test-hydration-schema-validator.sh — #2827 §A: validate-hydration-schema.sh
# correctly accepts a clean schema and refuses violations with named reasons.
#
# Test cases:
#   1. clean fixture (chorus:Hydratable + one subclass, every predicate has
#      a valid chorus:writeOwner)  →  rc=0
#   2. fixture with a hydratable predicate missing chorus:writeOwner
#      →  rc=1, log mentions "missing-writeOwner: chorus:badPred"
#   3. fixture with a hydratable predicate naming an unknown writer
#      (e.g. chorus:robot)  →  rc=1, log mentions "invalid-writeOwner"
#   4. real chorus.ttl (the one shipped in this card) → rc=0

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
VALIDATOR="$CHORUS_ROOT/platform/scripts/validate-hydration-schema.sh"

if [ ! -x "$VALIDATOR" ]; then
  echo "FAIL: validator not found / not executable at $VALIDATOR"
  exit 1
fi

FIXTURE_DIR=$(mktemp -d -t hydration-schema-test.XXXX)
cleanup() { rm -rf "$FIXTURE_DIR"; }
trap cleanup EXIT

write_fixture_clean() {
  cat > "$1" <<'EOF'
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix chorus: <https://jeffbridwell.com/chorus#> .

chorus:Hydratable a owl:Class .
chorus:Writer a owl:Class .
chorus:crawler a chorus:Writer .
chorus:enrichment a chorus:Writer .
chorus:writeOwner a owl:ObjectProperty .

chorus:Widget a owl:Class ;
    rdfs:subClassOf chorus:Hydratable .

chorus:widgetName a owl:DatatypeProperty ;
    rdfs:domain chorus:Widget ;
    rdfs:range xsd:string ;
    chorus:writeOwner chorus:crawler .

chorus:widgetScore a owl:DatatypeProperty ;
    rdfs:domain chorus:Widget ;
    rdfs:range xsd:integer ;
    chorus:writeOwner chorus:enrichment .
EOF
}

write_fixture_missing() {
  write_fixture_clean "$1"
  cat >> "$1" <<'EOF'

chorus:widgetBadPred a owl:DatatypeProperty ;
    rdfs:domain chorus:Widget ;
    rdfs:range xsd:string .
EOF
}

write_fixture_invalid() {
  write_fixture_clean "$1"
  cat >> "$1" <<'EOF'

chorus:widgetUnknownOwner a owl:DatatypeProperty ;
    rdfs:domain chorus:Widget ;
    rdfs:range xsd:string ;
    chorus:writeOwner chorus:robot .
EOF
}

# --- Test 1: clean fixture passes ---
echo "Test 1: clean fixture passes"
F1="$FIXTURE_DIR/clean.ttl"
write_fixture_clean "$F1"
if "$VALIDATOR" "$F1" >/dev/null 2>&1; then
  p "clean fixture → rc=0"
else
  f "clean fixture should pass but validator returned rc=$?"
fi

# --- Test 2: missing writeOwner is refused with named reason ---
echo "Test 2: missing writeOwner refused"
F2="$FIXTURE_DIR/missing.ttl"
write_fixture_missing "$F2"
OUT=$("$VALIDATOR" "$F2" 2>&1)
RC=$?
if [ "$RC" -eq 1 ] && echo "$OUT" | grep -q "missing-writeOwner: chorus:widgetBadPred"; then
  p "missing-writeOwner refused with named reason"
else
  f "expected rc=1 + 'missing-writeOwner: chorus:widgetBadPred', got rc=$RC: $OUT"
fi

# --- Test 3: invalid writeOwner is refused with named reason ---
echo "Test 3: invalid writeOwner refused"
F3="$FIXTURE_DIR/invalid.ttl"
write_fixture_invalid "$F3"
OUT=$("$VALIDATOR" "$F3" 2>&1)
RC=$?
if [ "$RC" -eq 1 ] && echo "$OUT" | grep -q "invalid-writeOwner: chorus:widgetUnknownOwner"; then
  p "invalid-writeOwner refused with named reason"
else
  f "expected rc=1 + 'invalid-writeOwner: chorus:widgetUnknownOwner', got rc=$RC: $OUT"
fi

# --- Test 4: real chorus.ttl passes ---
echo "Test 4: real chorus.ttl passes"
REAL="$CHORUS_ROOT/roles/silas/ontology/chorus.ttl"
if [ -f "$REAL" ]; then
  if "$VALIDATOR" "$REAL" >/dev/null 2>&1; then
    p "real chorus.ttl → rc=0"
  else
    f "real chorus.ttl should pass but validator returned rc=$?"
  fi
else
  echo "  SKIP: real chorus.ttl not at $REAL"
fi

# --- Test 5: missing TTL file → rc=2 ---
echo "Test 5: missing TTL file → rc=2"
"$VALIDATOR" "$FIXTURE_DIR/does-not-exist.ttl" >/dev/null 2>&1
RC=$?
if [ "$RC" -eq 2 ]; then
  p "missing TTL → rc=2 (usage error)"
else
  f "expected rc=2 for missing TTL, got rc=$RC"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
