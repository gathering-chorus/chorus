#!/usr/bin/env bats
# @test-type: contract — reads the model file and the deployer; no store, no writes
# @domain: services — the product domain this suite guards (#4334)
# #4187 — Service rows live in urn:chorus:domains:services, the domain graph, not
# the v1 catch-all. The shape declares it (the door reads and writes where the
# shape says) and the deployer seeds SERVICES_SET there. The reason they ever
# left (the harvester PUT-replacing the whole graph) was removed by #4089; this
# guard keeps the harvester on class-scoped replace so the reason cannot return.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
TTL="$REPO_ROOT/roles/silas/ontology/chorus.ttl"
DEPLOY="$REPO_ROOT/platform/services/athena-deploy/target/release/athena-deploy"
HARVEST="$REPO_ROOT/platform/scripts/service-harvest-load.sh"

# REWRITTEN 2026-09-19. This asserted that ServiceShape PINS
# chorus:instancesGraph to the services graph. That was the mechanism, not the
# property: later on this same card the pin was deliberately removed, because
# the services domain claims Service and athena-make's resolve_instances_graph
# (lib.rs:899) derives the identical string from that claim. A redundant pin
# reads like a decision, and that is how the stale catch-all pins survived long
# enough to become this card. The guard now asserts the PROPERTY - Service's
# home is the services graph - and that the catch-all pin is gone, which holds
# whether the home is pinned or derived.
@test "Service's home is the services domain graph, pinned or derived" {
  run python3 "$BATS_TEST_DIRNAME/4187-service-home-check.py" "$TTL" home "$REPO_ROOT"
  [ "$status" -eq 0 ]
}

# NEGATIVE PROOF for the rewrite above: the state the check exists to catch is a
# Service with NO home at all - no pin and no claim - which athena-make REFUSES
# (ADR-051 deleted the silent catch-all fallback). The fixture strips both
# halves and the same check must report homeless.
@test "NEGATIVE PROOF: a Service with no pin and no claim reads as homeless" {
  run python3 "$BATS_TEST_DIRNAME/4187-service-home-check.py" "$TTL" homeless-fixture
  [ "$status" -eq 0 ]
}

@test "the deployer seeds SERVICES_SET into the services domain graph by default" {
  # #4229 - the set is a manifest row now, not a shell variable. Same question:
  # services land in their own domain graph, never the catch-all.
  MAN="$BATS_TEST_DIRNAME/../config/domain-set-manifest.txt"
  run grep -c "^services|urn:chorus:domains:services|" "$MAN"
  [ "$output" -ge 1 ]
  # NEGATIVE PROOF: and no row of any set may point at the catch-all.
  run grep -c "|urn:chorus:instances|" "$MAN"
  [ "$output" -eq 0 ]
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
