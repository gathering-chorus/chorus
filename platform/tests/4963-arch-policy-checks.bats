#!/usr/bin/env bats
# @test-type: contract — the plan, its shape and the land's dry run are hermetic; the last case measures this tree against the live store (read-only).
# @domain: policies
# @card: #4963
# @owner: silas
#
# #4963 AC1 — each of the 13 arch rows carries checkKind, checkSpec, a baseline measured on
# main and a violationFixture; land-4963-rows sets them on the rows #4960 landed (13 changes,
# no adds, no deletes); `policy-validate --gate arch` runs all 13 and exits 0 at the baselines.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/data/policy-checks-4963.json"
PV="$ROOT/platform/scripts/policy-validate"
LAND="$ROOT/platform/scripts/land-4963-rows"
ARCH="arch-cmdb-service-registered arch-compute-decoupled-via-store arch-dagu-generated-pipelines arch-domain-directory-layout arch-harness-adapter-boundary arch-hook-single-responsibility arch-language-boundary arch-native-launchd-only arch-one-execution-path arch-single-binary-deploy arch-single-home-projection arch-three-generator-separation arch-verb-naming-and-atomic-core"

setup() { T="$BATS_TEST_TMPDIR"; }

@test "the plan is the 13 arch rules, each with a check kind, a spec, a measured baseline and a fixture on disk" {
  run python3 - "$CHECKS" "$ROOT" "$ARCH" <<'PY'
import json, os, sys
rows = json.load(open(sys.argv[1]))
want = sys.argv[3].split()
assert sorted(r["name"] for r in rows) == want, sorted(r["name"] for r in rows)
for r in rows:
    assert r["gate"] == "arch" and r["status"] == "ratchet", r["name"]
    assert r["checkKind"] in ("command", "sparql") and r["checkSpec"].strip(), r["name"]
    assert isinstance(r["baseline"], str) and r["baseline"].isdigit(), r["name"]
    assert os.path.isfile(os.path.join(sys.argv[2], r["violationFixture"])), r["violationFixture"]
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "every planned row conforms to the live PolicyShape" {
  run python3 "$LAND" --ttl "$T/rows.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-silas a c:Principal .\n' >> "$T/rows.ttl"
  run shacl validate --shapes "$ROOT/roles/wren/ontology/policies-4077.ttl" --data "$T/rows.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output" | grep resultMessage | head; return 1; }
}

@test "the dry run changes the 13 existing rows, adds and deletes nothing, writes nothing" {
  python3 - "$T/live.json" "$ARCH" <<'PY'
import json, sys
json.dump({"policies": [{"name": n, "gate": "arch", "comment": "words only"} for n in sys.argv[2].split()]}, open(sys.argv[1], "w"))
PY
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"0 add, 13 change, 0 delete"* ]] || { echo "$output" | tail -3; return 1; }
  [ ! -f "$T/runner.log" ] || return 1
}

@test "negative proof: a write the door refuses makes the apply exit 1" {
  printf '{"policies": []}' > "$T/live.json"
  printf '#!/bin/sh\necho "→ 422"\nexit 1\n' > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" LAND_SCOPE_WAIT_CMD=true run python3 "$LAND" --apply
  [ "$status" -eq 1 ] && [[ "$output" == *"REFUSED"* ]] || { echo "$output" | tail -3; return 1; }
}

@test "policy-validate --gate arch runs all 13 on this tree and exits 0 at the measured baselines" {
  python3 -c "import json; json.dump({'data': json.load(open('$CHECKS'))}, open('$T/p.json', 'w'))"
  run sh -c "cd '$ROOT' && CHORUS_HOME='$ROOT' POLICIES_JSON='$T/p.json' '$PV' --gate arch"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"13 checks, 0 above baseline, 0 could not run (gate arch)"* ]] || { echo "$output"; return 1; }
}
