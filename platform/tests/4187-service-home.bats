#!/usr/bin/env bats
# @test-type: contract — reads the model file and the deployer; no store, no writes
# #4187 — Service rows live in urn:chorus:domains:services, the domain graph, not
# the v1 catch-all. The shape declares it (the door reads and writes where the
# shape says) and the deployer seeds SERVICES_SET there. The reason they ever
# left (the harvester PUT-replacing the whole graph) was removed by #4089; this
# guard keeps the harvester on class-scoped replace so the reason cannot return.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
TTL="$REPO_ROOT/roles/silas/ontology/chorus.ttl"
DEPLOY="$REPO_ROOT/platform/scripts/athena-deploy-model.sh"
HARVEST="$REPO_ROOT/platform/scripts/service-harvest-load.sh"

@test "the Service shape declares its home as the services domain graph" {
  python3 - "$TTL" <<'PY'
import re, sys
t = open(sys.argv[1]).read()
i = t.index('chorus:ServiceShape a sh:NodeShape ;')
blk = t[i:t.index(' .\n', i)]
assert 'chorus:instancesGraph "urn:chorus:domains:services"' in blk, blk[:300]
assert 'chorus:instancesGraph "urn:chorus:instances"' not in blk
PY
}

@test "the deployer seeds SERVICES_SET into the services domain graph by default" {
  python3 - "$DEPLOY" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert 'SERVICES_GRAPH="${SERVICES_GRAPH:-urn:chorus:domains:services}"' in s
assert 'SERVICES_GRAPH="${SERVICES_GRAPH:-urn:chorus:instances}"' not in s
PY
}

@test "NEGATIVE PROOF of the precondition — the harvester replaces its own classes, never the whole graph" {
  # If this ever goes back to a graph-store PUT of the whole graph, Service rows
  # would be wiped every harvest cycle again, which is exactly why they fled.
  python3 - "$HARVEST" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert 'DELETE { GRAPH <%s> { ?s ?p ?o } }' in s and 'FILTER(?t' in s, 'class-scoped replace missing'
import re
puts = [l for l in s.splitlines() if re.search(r'-X\s+PUT\b', l) and 'graph=' in l]
assert not puts, puts
PY
}
