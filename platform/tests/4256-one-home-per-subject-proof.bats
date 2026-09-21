#!/usr/bin/env bats
# @test-type: unit — arq over a TriG fixture on disk; no service, no store, no network
# #4256 — chorus:gc-one-home-per-subject had no negative proof.
#
# Kade found it: the check claims chorus:provenRedRows 4 from the 2026-08-14
# sweep, and the only fixture it was ever run against holds 0 of them. It
# passed green every night because nothing in the tree could make it red — the
# hats fixture is one flat graph, and this check names two, so a flat graph
# can never satisfy it. A check that cannot go red is not a check (#3734).
#
# Offline: arq over a TriG fixture. No service, no store, no network.

ROOT="$BATS_TEST_DIRNAME/../.."
MODEL="$ROOT/designing/data/governance-check-instances.ttl"
FIXTURE="$ROOT/platform/tests/fixtures/one-home-per-subject-violations.trig"

setup() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — UNMEASURED here, not green"
  T="$(mktemp -d)"
  # the query travels with the rule: read it out of the model, never restate it
  python3 - "$MODEL" > "$T/q.rq" <<'PY'
import re, sys
t = open(sys.argv[1]).read()
m = re.search(r'chorus:gc-one-home-per-subject a chorus:GovernanceCheck ;[\s\S]*?"""([\s\S]*?)"""', t)
assert m, "gc-one-home-per-subject is not in the model any more — this test must fail, not vanish"
print(m.group(1))
PY
}

teardown() { rm -rf "$T"; }

rows() { arq --results csv --query "$T/q.rq" --data "$1" 2>/dev/null | tail -n +2 | grep -c . || true; }

@test "the claimed red count is the one the model declares, not a number in this file" {
  claimed=$(grep -A 8 'chorus:gc-one-home-per-subject a chorus:GovernanceCheck' "$MODEL" \
            | grep -oE 'chorus:provenRedRows [0-9]+' | grep -oE '[0-9]+')
  [ -n "$claimed" ]
  echo "$claimed" > "$T/claimed"
  [ "$claimed" -gt 0 ]
}

@test "NEGATIVE PROOF — the check goes RED on a fixture that violates it" {
  claimed=$(grep -A 8 'chorus:gc-one-home-per-subject a chorus:GovernanceCheck' "$MODEL" \
            | grep -oE 'chorus:provenRedRows [0-9]+' | grep -oE '[0-9]+')
  n=$(rows "$FIXTURE")
  [ "$n" -eq "$claimed" ]
}

@test "the check is SILENT on a graph with no subject in two homes" {
  cat > "$T/clean.trig" <<'TRIG'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
GRAPH <urn:chorus:ontology>  { chorus:a a chorus:Product . }
GRAPH <urn:chorus:instances> { chorus:b a chorus:Product . }
TRIG
  n=$(rows "$T/clean.trig")
  [ "$n" -eq 0 ]
}

@test "a fixture that only pairs shapes, without two homes, does not fire it" {
  # the #3734 trap: a check matching on shape rather than on the violation
  cat > "$T/shape-only.trig" <<'TRIG'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .
GRAPH <urn:chorus:ontology>  { chorus:x a chorus:Product ; rdfs:label "x" . }
GRAPH <urn:chorus:instances> { chorus:x rdfs:comment "same subject, no rdf:type here" . }
TRIG
  n=$(rows "$T/shape-only.trig")
  [ "$n" -eq 0 ]
}
