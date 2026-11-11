#!/usr/bin/env bats
# @test-type: unit — runs the shipped PrincipalShape rule over a fixture with Jena shacl; no store, no network
# @domain: identity
# @card: #4942 · owner: silas
# Checks the rule in roles/silas/ontology/security-model-3618.ttl (named in full so werk-test selects this suite).
#
# #4942 AC1 — an agent principal runs as its own macOS account: hostAccount must be
# chorus-<name>. On #4773 a model land set the agents to "jeffbridwell" and locked
# every role out with no deploy-time error. The rule is read from the shipped
# ontology (never copied), so a renamed or deleted rule fails here. Services and
# persons are out of scope (10 live services run as jeffbridwell; 2 persons have none).
# A bats suite, not a .test.sh: only bats/cargo/jest record case events, and the
# land's prove reads cases (#4942 run 2 was unmeasured as a .test.sh).

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export TMPD="$(mktemp -d)"
  command -v shacl >/dev/null || { echo "Jena shacl not on PATH" >&2; return 1; }
  awk '/^chorus:AgentRunsAsItsOwnAccount a sh:SPARQLConstraint/,/""" \./' "$ROOT/roles/silas/ontology/security-model-3618.ttl" > "$TMPD/rule.ttl"
  { printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    printf 'chorus:AgentHostAccountTest a sh:NodeShape ; sh:targetClass chorus:Principal ; sh:sparql chorus:AgentRunsAsItsOwnAccount .\n'
    cat "$TMPD/rule.ttl"; } > "$TMPD/shapes.ttl"
  shacl validate --shapes "$TMPD/shapes.ttl" --data "$ROOT/platform/tests/fixtures/agent-host-account-4942.ttl" > "$TMPD/report.ttl" 2>"$TMPD/err" || true
  sparql --data "$TMPD/report.ttl" --results TSV 'PREFIX sh: <http://www.w3.org/ns/shacl#>
    SELECT DISTINCT ?f WHERE { ?r a sh:ValidationResult ; sh:focusNode ?f }' | tail -n +2 | sed 's|.*#principal-||; s|>||' | sort > "$TMPD/refused"
}
teardown_file() { rm -rf "$TMPD"; }

refused() { grep -qx "$1" "$TMPD/refused"; }

@test "the rule is in the shipped security model and shacl ran clean" {
  [ -s "$TMPD/rule.ttl" ]
  [ ! -s "$TMPD/err" ]
}
@test "NEGATIVE PROOF: an agent set to jeffbridwell (the #4773 shape) is refused" { refused wrongv; }
@test "NEGATIVE PROOF: an agent with no hostAccount is refused" { refused nohost; }
@test "NEGATIVE PROOF: an agent on another agent's account is refused" { refused other; }
@test "an agent on its own account, a service on jeffbridwell and a person with none conform" {
  ! refused good; ! refused svc; ! refused person
}
@test "exactly the three built violations, no more" {
  [ "$(tr '\n' ' ' < "$TMPD/refused")" = "nohost other wrongv " ]
}
