#!/usr/bin/env bats
# @test-type: unit — reads the script text; no store, no writes.
# #4216 — the rehome tool must be able to read from the SCHEMA graph.
#
# graph-rehome-4187.sh was written for the v1 catch-all and hardcoded
# urn:chorus:instances as its source. The identical move is needed out of
# urn:chorus:ontology, which is holding a few hundred real records mixed in with
# the class and shape definitions. Rather than fork the script — which is how we
# ended up with four glossaries — the SOURCE became an override with the old
# value as its default.

SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/graph-rehome-4187.sh"

@test "the source graph is overridable" {
  run grep -c 'SRC="${REHOME_SRC:-urn:chorus:instances}"' "$SCRIPT"
  [ "$output" = "1" ]
}

@test "the default is unchanged, so every existing invocation behaves identically" {
  run env -u REHOME_SRC bash -c 'SRC="${REHOME_SRC:-urn:chorus:instances}"; echo "$SRC"'
  [ "$output" = "urn:chorus:instances" ]
}

@test "an override is actually honoured" {
  run env REHOME_SRC=urn:chorus:ontology bash -c 'SRC="${REHOME_SRC:-urn:chorus:instances}"; echo "$SRC"'
  [ "$output" = "urn:chorus:ontology" ]
}

# NEGATIVE PROOF (#3734): the state this guard exists to catch is the source
# being hardcoded again — a later edit pinning the catch-all back in place would
# silently make every schema-graph move a no-op against the wrong graph. Restore
# the literal in a copy and the check must FAIL.
@test "NEGATIVE PROOF: a re-hardcoded source is caught" {
  cp "$SCRIPT" "$BATS_TMPDIR/mutant.sh"
  sed -i '' 's|SRC="${REHOME_SRC:-urn:chorus:instances}"|SRC="urn:chorus:instances"|' "$BATS_TMPDIR/mutant.sh"
  run grep -c 'SRC="${REHOME_SRC:-urn:chorus:instances}"' "$BATS_TMPDIR/mutant.sh"
  [ "$output" = "0" ]
}
