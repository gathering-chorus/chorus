#!/usr/bin/env bats
# @test-type: integration
# @domain: services
# @card: #5042 AC2 · owner: kade
#
# #5045's land stopped when the door refused a row its dry run had planned (Wren, 2026-11-27
# 10:14). The offline suite (generated-api-service-rows.bats) holds the PLAN; this holds that the
# door TAKES it: platform/scripts/land-5042-rows --apply on a private
# door (its own in-memory copy of the store, never prod), then a second run plans nothing as the
# door serves the rows back. The copy makes the caller the owner of the two hosting Domain rows
# and grants it the services graph, so the writes Wren makes at the land are made here by the
# role the test runs as, with the same bodies. The runner cannot be pointed at a private door
# from a role's account (a call forwards to the shared runner, which writes to the real door), so
# the writer here makes the runner's own door call (chorus-runner lib.rs athena_call: POST
# /v1/<domain>/<class>, PATCH /v1/<domain>/<class>/<id>, the fields JSON as the body) with the
# role's own credential, as 4522-create-linked-door.bats does.
# NEGATIVE: a body the door refuses is refused here too (a patch naming an off-model field), so
# a green run is the door's acceptance, never a door that takes anything.

setup_file() {
  if [ "${RUN_INTEGRATION:-}" = "true" ] && [ -z "${OWL_URL:-}" ]; then
    source "$BATS_TEST_DIRNAME/lib/test-store.sh"
    ME="$(id -un | sed 's/^chorus-//')"
    export PRIVATE_DOOR_PREP="PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX acl: <http://www.w3.org/ns/auth/acl#>
DELETE { GRAPH <urn:chorus:domains:domains> { ?d c:ownedBy ?o } } WHERE { GRAPH <urn:chorus:domains:domains> { VALUES ?d { c:principles c:awareness } ?d c:ownedBy ?o } } ;
INSERT DATA { GRAPH <urn:chorus:domains:domains> { c:principles c:ownedBy c:principal-$ME . c:awareness c:ownedBy c:principal-$ME . }
  GRAPH <urn:chorus:domains:security> {
    c:permission-probe-5042-services a c:Permission ; c:agent c:principal-$ME ; c:accessTo <urn:chorus:domains:services> ; c:mode acl:Write .
    c:permission-probe-5042-domains a c:Permission ; c:agent c:principal-$ME ; c:accessTo <urn:chorus:domains:domains> ; c:mode acl:Write . } }"
    private_door || { export PRIVATE_DOOR_WHY="$TEST_STORE_WHY"; return 0; }
  fi
}
teardown_file() {
  # a door that never started left nothing to stop (its dataset was refused, not made)
  [ -z "${PRIVATE_DOOR_WHY:-}" ] || return 0
  source "$BATS_TEST_DIRNAME/lib/test-store.sh"
  private_door_stop
}
setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true"
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  [ -z "${PRIVATE_DOOR_WHY:-}" ] || skip "UNMEASURED — no private door: $PRIVATE_DOOR_WHY"
  [ -n "${PRIVATE_DOOR_DS:-}" ] || skip "UNMEASURED — this suite writes, so it runs only on its private door"
  case "${OWL_URL:-}" in ""|*:3360*) skip "refuses to write to the canonical store" ;; esac
  ME="$(id -un | sed 's/^chorus-//')"
  # run 1 (11-27): the identity command exited 3 in the pipeline and setup failed before it could
  # skip; a missing credential is UNMEASURED, never a red and never a pass
  TS="$("$REPO/platform/scripts/chorus-identity-token" "$ME" 2>/dev/null || true)"
  [ -n "$TS" ] || skip "UNMEASURED — no credential for $ME"
  RUNNER="$BATS_TEST_TMPDIR/door-writer"
  cat > "$RUNNER" <<'PY'
#!/usr/bin/env python3
# the runner's athena-add / athena-set door call, to $OWL_URL only (see the header)
import json, os, sys, urllib.request, urllib.error
verb, args = sys.argv[2], dict(a.split("=", 1) for a in sys.argv[3:])
door = os.environ["OWL_URL"]
assert ":3360" not in door, "refuses to write to the canonical store"
path = f"/v1/{args['domain']}/{args['class']}" + (f"/{args['id']}" if verb == "athena-set" else "")
req = urllib.request.Request(door + path, data=args["fields"].encode(), method={"athena-add": "POST", "athena-set": "PATCH"}[verb],
                             headers={"Authorization": "Bearer " + os.environ["TS"], "Content-Type": "application/json"})
try:
    with urllib.request.urlopen(req, timeout=30) as r:
        print(f"{verb} {path} → {r.status} {r.read().decode()[:300]}")
except urllib.error.HTTPError as e:
    print(f"{verb} {path} → {e.code} {e.read().decode()[:300]}"); sys.exit(1)
PY
  chmod +x "$RUNNER"
  export TS
}

land() { CHORUS_RUNNER="$RUNNER" OWL_URL="$OWL_URL" run python3 "$REPO/platform/scripts/land-5042-rows" --as "$ME" "$@"; }

@test "#5042 the door takes every planned write, hosts edge included, and a second run plans nothing" {
  land
  echo "$output" | grep -q '2 add, 2 change, 0 delete' || { echo "the private copy is not the state the land meets: $output"; return 1; }
  land --apply
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q 'applied' || { echo "$output"; return 1; }
  land
  echo "$output" | grep -q '0 add, 0 change, 0 delete; 2 same' || { echo "the door does not serve back what it took: $output"; return 1; }
  run curl -s --max-time 15 "$OWL_URL/v1/domains/domains/principles"
  echo "$output" | grep -q 'service-principles-api' || { echo "principles hosts nothing: $output"; return 1; }
  run curl -s --max-time 15 "$OWL_URL/v1/domains/domains/awareness"
  echo "$output" | grep -q 'service-pulse-api' || { echo "awareness hosts nothing: $output"; return 1; }
}

@test "#5042 NEGATIVE: the private door refuses a body the door refuses (an off-model field)" {
  run "$RUNNER" call athena-set domain=domains class=domains id=principles 'fields={"notAField5042":"x"}'
  echo "$output" | grep -qE ' → (200|201|204)' && { echo "the door took an off-model field: $output"; return 1; }
  echo "$output" | grep -qE ' → 4[0-9][0-9]' || { echo "no refusal from the door: $output"; return 1; }
}
