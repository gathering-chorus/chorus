#!/usr/bin/env bats
# @test-type: security — asserts the gate's verdict against fixture graphs; no
# credential is exercised beyond the store's own read auth.
#
# #3788 — the allow-set gate asserts NAMED PRESENCE, not a count.
#
# On 2026-08-07 the count version passed while every human and every agent was
# locked out of governed writes. The graph held three principals — the workers —
# because seven humans and agents had been deleted by a retirement that re-fired.
# Three is non-zero. The gate said OK.
#
# A count cannot distinguish "the people who need to work can work" from "three
# background jobs survived". That is the two-states defect (#3734) in a check I
# wrote the day before, which is the whole reason this file exists: the fix is
# not more checks, it is checks that can fail on the state they exist to catch.
#
# The failure state is RECONSTRUCTED here rather than described. A test that
# asserts against a graph nobody built proves the gate compiles.

setup() {
  ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  GATE="$ROOT/platform/scripts/chorus-allow-set-gate"
  command -v curl >/dev/null || skip "curl absent"
  # shellcheck disable=SC1091
  set +u; . "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null; set -u
  [ -n "${FUSEKI_ADMIN_USER:-}" ] || skip "no store credentials here"
  UPD="${CHORUS_FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
  G="urn:chorus:test-3788-$$"
  P="https://jeffbridwell.com/chorus#principal"
  W="https://jeffbridwell.com/chorus#webId"
  A="https://jeffbridwell.com/chorus#Principal"
}

teardown() {
  [ -n "${G:-}" ] && curl -s -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" -X POST "$UPD" \
    --data-urlencode "update=DROP SILENT GRAPH <$G>" -o /dev/null 2>/dev/null || true
}

seed() {
  local triples="$1"
  curl -s -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" -X POST "$UPD" \
    --data-urlencode "update=DROP SILENT GRAPH <$G>; INSERT DATA { GRAPH <$G> { $triples } }" -o /dev/null
}

principal() { printf '<%s-%s> a <%s> ; <%s> "https://id.example.test/%s/profile/card#me" . ' "$P" "$1" "$A" "$W" "$1"; }

@test "a graph with every required principal PASSES" {
  seed "$(principal jeff)$(principal silas)$(principal wren)$(principal kade)"
  CHORUS_ALLOW_SET_GRAPH="$G" run "$GATE"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: workers-only — the 2026-08-07 state — is REFUSED" {
  # The exact shape of the incident: non-empty, and nobody who matters is in it.
  seed "$(principal crawler-index)$(principal reindex-worker)$(principal embed-worker)"
  CHORUS_ALLOW_SET_GRAPH="$G" run "$GATE"
  [ "$status" -eq 1 ]
  [[ "$output" == *"cannot write"* ]]
}

@test "NEGATIVE PROOF: one missing person is enough to refuse" {
  # Partial loss is the likelier failure than total loss, and the easier one to
  # wave through. Three of four present must still be a refusal.
  seed "$(principal jeff)$(principal silas)$(principal wren)"
  CHORUS_ALLOW_SET_GRAPH="$G" run "$GATE"
  [ "$status" -eq 1 ]
  [[ "$output" == *"kade"* ]]
}

@test "NEGATIVE PROOF: an empty graph is still refused (unregressed from #3785)" {
  seed "<urn:x> <urn:y> <urn:z> ."
  CHORUS_ALLOW_SET_GRAPH="$G" run "$GATE"
  [ "$status" -eq 1 ]
}

@test "the required set is CONFIG — a narrower requirement passes the same graph" {
  # Proves the refusals above come from the named check rather than from the gate
  # refusing everything: same data, smaller requirement, different verdict.
  seed "$(principal jeff)$(principal silas)"
  CHORUS_ALLOW_SET_GRAPH="$G" CHORUS_ALLOW_SET_REQUIRED="jeff silas" run "$GATE"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: could-not-ask is refused differently from nobody-allowed" {
  # exit 2, not 1 — a store that cannot answer is not a store answering "none",
  # and collapsing them is the confusion this gate exists to end.
  CHORUS_FUSEKI_QUERY="http://127.0.0.1:9/nope" CHORUS_ALLOW_SET_GRAPH="$G" run "$GATE"
  [ "$status" -eq 2 ]
}
