#!/usr/bin/env bash
# @test-type: unit — runs the shipped PrincipalShape rule over a fixture with Jena shacl; no store, no network
# @domain: identity
# @card: #4942 · owner: silas
#
# #4942 AC1 — an agent principal runs as its own macOS account: hostAccount must be
# chorus-<name>. On #4773 a model land set the agents to "jeffbridwell" and locked
# every role out with no deploy-time error. The rule is read from the shipped
# ontology (never copied), so a renamed or deleted rule fails here. Services and
# persons are out of scope (10 live services run as jeffbridwell; 2 persons have none).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ONT="$ROOT/roles/silas/ontology"
FIX="$ROOT/platform/tests/fixtures/agent-host-account-4942.ttl"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
command -v shacl >/dev/null || { echo "FAIL: Jena shacl not on PATH"; exit 1; }
# only the rule's own node shape: the full PrincipalShape also demands label, webId
# and the rest, which this fixture does not carry on purpose
awk '/^chorus:AgentRunsAsItsOwnAccount a sh:SPARQLConstraint/,/""" \./' "$ONT/security-model-3618.ttl" > "$TMP/rule.ttl"
[ -s "$TMP/rule.ttl" ] || { echo "FAIL: AgentRunsAsItsOwnAccount is not in security-model-3618.ttl"; exit 1; }
{ printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
  printf 'chorus:AgentHostAccountTest a sh:NodeShape ; sh:targetClass chorus:Principal ; sh:sparql chorus:AgentRunsAsItsOwnAccount .\n'
  cat "$TMP/rule.ttl"; } > "$TMP/shapes.ttl"
shacl validate --shapes "$TMP/shapes.ttl" --data "$FIX" > "$TMP/report.ttl" 2>"$TMP/err" || true
[ -s "$TMP/err" ] && { echo "FAIL: shacl errored:"; cat "$TMP/err"; exit 1; }
got=$(sparql --data "$TMP/report.ttl" --results TSV 'PREFIX sh: <http://www.w3.org/ns/shacl#>
  SELECT DISTINCT ?f WHERE { ?r a sh:ValidationResult ; sh:focusNode ?f }' | tail -n +2 | sed 's|.*#principal-||; s|>||' | sort | tr '\n' ' ')
want="nohost other wrongv "
pass=0; fail=0
for p in wrongv nohost other; do
  case " $got" in *" $p "*) echo "PASS principal-$p is refused"; pass=$((pass+1));; *) echo "FAIL principal-$p should be refused"; fail=$((fail+1));; esac
done
for p in good svc person; do
  case " $got" in *" $p "*) echo "FAIL principal-$p should conform"; fail=$((fail+1));; *) echo "PASS principal-$p conforms"; pass=$((pass+1));; esac
done
[ "$got" = "$want" ] || { echo "FAIL violations were [$got], want [$want]"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
