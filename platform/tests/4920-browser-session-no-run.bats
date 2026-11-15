#!/usr/bin/env bats
# @test-type: unit — Jena's shacl on fixtures with the real OpenSessionHasOneLiveRun constraint; no store
# @domain: identity
# @card: #4920 · owner: silas
# Checks roles/silas/ontology/session-model-4302.ttl (chorus:OpenSessionHasOneLiveRun).
#
# #4920 — 123 SHACL violations (2026-11-15 census): every open browser sign-in failed "an open
# session has exactly one live run". A browser sign-in (person_session.py) has no run by
# design: no SessionRun in the store has ever had runOf a browser session (measured 13:26).
# The rule is for sessions that run an agent.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  # the constraint as the model holds it (read, not retyped)
  python3 - "$ROOT/roles/silas/ontology/session-model-4302.ttl" > "$T/constraint.ttl" <<'EOF'
import re, sys
s = open(sys.argv[1]).read()
m = re.search(r'chorus:OpenSessionHasOneLiveRun a sh:SPARQLConstraint ;.*?""" \.', s, re.S)
print(m.group(0) if m else "")
EOF
  [ -s "$T/constraint.ttl" ] || { echo "no OpenSessionHasOneLiveRun in the model"; return 1; }
}

validate() { # <data turtle> -> conforms true|false
  { printf '%s\n' '@prefix sh: <http://www.w3.org/ns/shacl#> .' '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
      'chorus:SessionShape a sh:NodeShape ; sh:targetClass chorus:Session ; sh:sparql chorus:OpenSessionHasOneLiveRun .'
    cat "$T/constraint.ttl"; } > "$T/shapes.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n%s\n' "$1" > "$T/data.ttl"
  shacl validate --shapes "$T/shapes.ttl" --data "$T/data.ttl" 2>&1 | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'
}

@test "an open browser sign-in with no run conforms" {
  [ "$(validate 'chorus:s1 a chorus:Session ; chorus:sessionState "open" ; chorus:channel "browser" .')" = true ]
}

# NEGATIVE PROOF: an agent session still needs its one live run
@test "an open pane session with no run, or with two live runs, fails" {
  [ "$(validate 'chorus:s1 a chorus:Session ; chorus:sessionState "open" ; chorus:channel "pane" .')" = false ] || return 1
  [ "$(validate 'chorus:s1 a chorus:Session ; chorus:sessionState "open" ; chorus:channel "pane" .
chorus:r1 chorus:runOf chorus:s1 . chorus:r2 chorus:runOf chorus:s1 .')" = false ] || return 1
  [ "$(validate 'chorus:s1 a chorus:Session ; chorus:sessionState "open" ; chorus:channel "pane" .
chorus:r1 chorus:runOf chorus:s1 .')" = true ] || return 1
}
