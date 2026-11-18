#!/usr/bin/env bats
# @test-type: unit + e2e — athena-validate's owner rule on a scratch store; after the land, the live validator
# @domain: domains
# @card: #5002
#
# #5002 AC1 (Jeff 11-18 10:04 "yes now"; was 14): the 37 instance classes whose rows carry
# ownedBy declare what an owner is (ownedBy sh:class Principal), required on the 22 fully
# owned, optional on the 15 partial, no backfill. athena-validate's owner-rule-missing names
# each class still silent with its count of owned rows, follows rdfs:subClassOf (a superclass
# shape governs its subclasses), and after the land names only the remainder.
#
# Covers: platform/services/athena-validate/src/ported.rs
# Covers: roles/silas/ontology/chorus.ttl

setup() { ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"; }

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

# The remainder, named and kept out of AC1: model terms that carry a stray owner (owl:Class,
# skos:ConceptScheme) and the two deprecated role classes (AgentRole, HumanRole; use roleKind).
REMAINDER="Class ConceptScheme AgentRole HumanRole"

# $1 validator output: every owner-rule-missing class is in the remainder.
only_remainder() {
  python3 - "$1" "$REMAINDER" <<'PY'
import sys
rest = set(sys.argv[2].split())
named = [l.split("|")[2] for l in sys.argv[1].splitlines() if l.startswith("graph-issue|owner-rule-missing|") and "UNMEASURED" not in l]
if any("UNMEASURED" in l and "owner-rule-missing" in l for l in sys.argv[1].splitlines()):
    print("owner-rule-missing did not run"); sys.exit(1)
extra = sorted(set(named) - rest)
print("owner-rule-missing: " + (", ".join(extra) + " still silent" if extra else f"only the remainder ({', '.join(sorted(set(named)))})"))
sys.exit(1 if extra else 0)
PY
}

@test "AC1 the owner rule counts each silent class and follows subClassOf (scratch store, with its negative proof)" {
  run bash -c "cd '$ROOT/platform/services/athena-validate' && cargo test --release --quiet --test owner_rule_5002 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. 1 passed" || return 1
}

@test "AC1 after the land: the live validator names only the remainder" {
  in_werk && skip "after-land check: a card's werk has not landed"
  run bash -c "'$HOME/.chorus/bin/athena-validate' --store-only 2>&1"
  run only_remainder "$output"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC1 NEGATIVE PROOF: a silent class outside the remainder is red; remainder only passes; an unmeasured run is red" {
  run only_remainder $'graph-issue|owner-rule-missing|Class|1\ngraph-issue|owner-rule-missing|HumanRole|1'
  [ "$status" -eq 0 ] || return 1
  run only_remainder $'graph-issue|owner-rule-missing|Class|1\ngraph-issue|owner-rule-missing|Endpoint|222'
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Endpoint still silent"* ]] || return 1
  run only_remainder 'graph-issue|owner-rule-missing|UNMEASURED|store unreachable'
  [ "$status" -eq 1 ] || return 1
}
