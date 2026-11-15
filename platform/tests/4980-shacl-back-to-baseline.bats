#!/usr/bin/env bats
# @test-type: unit — Jena's shacl on fixtures with the real DomainShape property; no store
# @domain: persistence
# @card: #4980 · owner: silas
# Checks roles/silas/ontology/chorus.ttl (DomainShape, the ^hasDomain property).
#
# #4980 AC2 — the SHACL rise. The census by class and field (scratch run, prod read-only,
# 2026-11-15 09:25) put 9,492 of 16,247 violations on one line: Domain ^hasDomain sh:class.
# DomainShape asked that EVERY row naming a domain through hasDomain be a Product, so each
# session, alert or sensor run written with a domain added one more violation to that
# domain. The intent (ADR-041, #4288) is "at least one composing Product": a qualified
# value shape. The rows are right; the shape was wrong.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  # the DomainShape's ^hasDomain property, as chorus.ttl holds it (read, not retyped)
  PROP="$(python3 - "$ROOT/roles/silas/ontology/chorus.ttl" <<'EOF'
import re, sys
s = open(sys.argv[1]).read()
m = re.search(r"sh:property \[ sh:path \[ sh:inversePath chorus:hasDomain \] ;.*?\] \.", s, re.S)
print(m.group(0)[:-1].split("#")[0].strip() if m else "")
EOF
)"
  [ -n "$PROP" ] || { echo "no ^hasDomain property in chorus.ttl"; return 1; }
}

validate() { # <shape property text> <data turtle> -> conforms true|false
  cat > "$T/shapes.ttl" <<EOF
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:DomainShape a sh:NodeShape ; sh:targetClass chorus:Domain ;
    $1 .
EOF
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n%s\n' "$2" > "$T/data.ttl"
  shacl validate --shapes "$T/shapes.ttl" --data "$T/data.ttl" 2>/dev/null | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'
}

ROWS='chorus:infra a chorus:Domain .
chorus:chorus a chorus:Product ; chorus:hasDomain chorus:infra .
chorus:session-1 a chorus:Session ; chorus:hasDomain chorus:infra .
chorus:alert-1 a chorus:Alert ; chorus:hasDomain chorus:infra .'

@test "a domain with one composing Product conforms, though sessions and alerts also name it" {
  [ "$(validate "$PROP" "$ROWS")" = true ] || { echo "$PROP"; return 1; }
}

# NEGATIVE PROOF: a domain no Product composes still fails (the rule is kept, not dropped)
@test "a domain that only sessions and alerts name, with no Product, fails" {
  [ "$(validate "$PROP" "chorus:infra a chorus:Domain .
chorus:session-1 a chorus:Session ; chorus:hasDomain chorus:infra .")" = false ] || return 1
  [ "$(validate "$PROP" "chorus:infra a chorus:Domain .")" = false ] || return 1
}

# NEGATIVE PROOF: the shape as it was on main fails the same rows (each non-Product a violation)
@test "the old sh:class form fails a domain that sessions also name" {
  old='sh:property [ sh:path [ sh:inversePath chorus:hasDomain ] ; sh:minCount 1 ; sh:class chorus:Product ]'
  [ "$(validate "$old" "$ROWS")" = false ] || return 1
}
