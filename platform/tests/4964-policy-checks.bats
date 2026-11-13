#!/usr/bin/env bats
# @test-type: contract — the plan, its shape and the land's dry run are hermetic; the last case measures this tree against the live store and board (read-only).
# @domain: policies
# @card: #4964
# @owner: silas
#
# #4964 AC1 — each of the 15 quality, data, product and code rows carries checkKind, checkSpec,
# a baseline measured on main and a violationFixture; land-4964-rows sets them on the rows
# #4960 landed and moves #4963's 13 to the one counter (28 changes, no adds, no deletes);
# policy-validate runs all 15 and exits 0 at the baselines.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/domains/policies/policy-checks-4964.json"
ARCH_CHECKS="$ROOT/designing/domains/policies/policy-checks-4963.json"
PV="$ROOT/platform/scripts/policy-validate"
LAND="$ROOT/platform/scripts/land-4964-rows"
RULES="code-object-authz-and-secret-mask code-test-side-effect-isolation data-domain-is-punned-owl-class data-no-hardcoded-graph-mappings data-property-key-registered data-single-graph-multi-lens prod-blast-radius-before-pull prod-interactive-skill-5s-budget prod-private-by-default-graduation prod-service-design-html-first qual-domain-fitness-functions qual-golden-api-regression qual-negative-proof-for-gates qual-no-silent-pipeline-drops qual-warning-ratchet-downward"

setup() { T="$BATS_TEST_TMPDIR"; }

@test "the plan is the 15 rules, each with its gate, a check kind, a spec, a measured baseline and a fixture on disk" {
  run python3 - "$CHECKS" "$ROOT" "$RULES" <<'PY'
import json, os, sys
rows = json.load(open(sys.argv[1]))
assert sorted(r["name"] for r in rows) == sys.argv[3].split(), sorted(r["name"] for r in rows)
gate = {"qual": "quality", "data": "data", "prod": "product", "code": "code"}
for r in rows:
    assert r["gate"] == gate[r["name"].split("-")[0]] and r["status"] == "ratchet", r["name"]
    assert r["checkKind"] in ("command", "sparql") and r["checkSpec"].strip(), r["name"]
    assert isinstance(r["baseline"], str) and r["baseline"].isdigit(), r["name"]
    assert os.path.isfile(os.path.join(sys.argv[2], r["violationFixture"])), r["violationFixture"]
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "the 13 architecture rows run the one counter now; arch-policy-count is gone" {
  [ ! -e "$ROOT/platform/scripts/arch-policy-count" ] || return 1
  run python3 -c "
import json; r = json.load(open('$ARCH_CHECKS'))
bad = [x['name'] for x in r if x['checkKind'] == 'command' and 'platform/scripts/policy-count;' not in x['checkSpec']]
print(bad); assert not bad"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "every planned row conforms to the live PolicyShape" {
  run python3 "$LAND" --ttl "$T/rows.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-silas a c:Principal .\n' >> "$T/rows.ttl"
  run shacl validate --shapes "$ROOT/roles/wren/ontology/policies-4077.ttl" --data "$T/rows.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output" | grep resultMessage | head; return 1; }
}

@test "the dry run changes the 28 existing rows, adds and deletes nothing, writes nothing" {
  python3 - "$T/live.json" "$CHECKS" "$ARCH_CHECKS" <<'PY'
import json, sys
names = [r["name"] for p in sys.argv[2:] for r in json.load(open(p))]
json.dump({"policies": [{"name": n, "comment": "words only"} for n in names]}, open(sys.argv[1], "w"))
PY
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"0 add, 28 change, 0 delete"* ]] || { echo "$output" | tail -3; return 1; }
  [ ! -f "$T/runner.log" ] || return 1
}

@test "negative proof: a write the door refuses makes the apply exit 1" {
  printf '{"policies": []}' > "$T/live.json"
  printf '#!/bin/sh\necho "→ 422"\nexit 1\n' > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" LAND_SCOPE_WAIT_CMD=true run python3 "$LAND" --apply
  [ "$status" -eq 1 ] && [[ "$output" == *"REFUSED"* ]] || { echo "$output" | tail -3; return 1; }
}

@test "policy-validate runs all 15 on this tree and exits 0 at the measured baselines" {
  # the board's token lives on canonical (a werk has no .env): its cards CLI, as the counter uses in prod
  CANON="${CHORUS_HOME:-$ROOT}"
  python3 -c "import json; json.dump({'data': json.load(open('$CHECKS'))}, open('$T/p.json', 'w'))"
  run sh -c "cd '$ROOT' && CARDS='$CANON/platform/scripts/cards' CHORUS_HOME='$ROOT' POLICIES_JSON='$T/p.json' '$PV'"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"15 checks, 0 above baseline, 0 could not run"* ]] || { echo "$output"; return 1; }
}
