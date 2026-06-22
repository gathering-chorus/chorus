#!/usr/bin/env bats
# @test-type: unit — reads source text only; no Fuseki, no live service, brings its own world.
# #3560 — bounded delete guard in tag-tests-domain.py.
# Retirement gate (the raw unguarded DELETE can't come back) + behavior proof
# (the guard refuses system/portfolio graphs). Hermetic: reads source only,
# never contacts Fuseki or any live service.

setup() { ROOT="$(git rev-parse --show-toplevel)"; SCRIPT="$ROOT/platform/scripts/tag-tests-domain.py"; }

@test "the only DELETE WHERE is the guarded one, and main() routes through clear_graph" {
  # exactly one executable DELETE (the f-string form `DELETE WHERE {{`), inside clear_graph().
  # (the guard's prose comment says "DELETE WHERE { GRAPH" with single braces — not matched.)
  run bash -c "grep -c 'DELETE WHERE {{' '$SCRIPT'"
  [ "$output" -eq 1 ]
  # the retired form — an unguarded DELETE against the {DG} constant — must be GONE
  run bash -c "grep -nE 'DELETE WHERE [{][{] GRAPH <[{]DG[}]>' '$SCRIPT'"
  [ "$status" -ne 0 ]
  # the surviving DELETE is the guard's, parameterised on the validated {dg}
  run bash -c "grep -q 'DELETE WHERE {{ GRAPH <{dg}>' '$SCRIPT'"
  [ "$status" -eq 0 ]
  # main() routes the clear through the guarded path
  run bash -c "grep -q 'clear_graph(DG)' '$SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "the guard regex refuses system/portfolio graphs and allows domain graphs" {
  run python3 - "$SCRIPT" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
m = re.search(r"_DOMAIN_GRAPH = re\.compile\(r'([^']+)'\)", src)
assert m, "guard regex _DOMAIN_GRAPH not found — guard removed?"
pat = re.compile(m.group(1))
refuse = ["urn:chorus:instances", "urn:gathering:photos", "urn:jb:sexuality",
          "urn:gathering:documents", "", "urn:chorus:ontology"]
allow  = ["urn:chorus:domain:tests", "urn:chorus:domain:photos", "urn:chorus:domain:cards"]
bad_allow  = [x for x in refuse if pat.match(x)]
bad_refuse = [x for x in allow  if not pat.match(x)]
assert not bad_allow,  f"guard LET THROUGH system/portfolio graphs: {bad_allow}"
assert not bad_refuse, f"guard BLOCKED valid domain graphs: {bad_refuse}"
print("ok")
PY
  [ "$status" -eq 0 ]
  [[ "$output" == *ok* ]]
}
