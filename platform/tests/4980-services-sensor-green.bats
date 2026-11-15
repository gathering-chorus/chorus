#!/usr/bin/env bats
# @test-type: integration — reads Loki (sensor.run.ended) for the live case; fixtures and the map file for the rest
# @domain: services
# @card: #4980 · owner: silas
# Checks platform/config/service-instance-map.json and platform/scripts/land-4980-rows.
#
# #4980 AC1 — the services sensor ran red every hour on 2026-11-15 (29 findings, from
# /tmp/service-harvest.log): 26 dead ServiceInstance rows (22 leaked quartet-test probes, 4
# gone units; deleted 09:05 on Jeff's word), 2 running units the map did not name
# (com.chorus.clearing-v2, com.chorus.go) and 1 mapping for a retired unit
# (com.chorus.nightly-suites, #4562). The map names the two and drops the one; after the
# land, land-4980-rows runs the sensor once and refuses unless it ends green; the after-land
# check reads that newest run from Loki (an http check on the card, never a skip in a werk).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MAP="$ROOT/platform/config/service-instance-map.json"
}

@test "the map names clearing-v2 and go, and no longer the retired nightly-suites" {
  python3 - "$MAP" <<'EOF' || return 1
import json, sys
m = json.load(open(sys.argv[1]))
assert m.get("com.chorus.clearing-v2") == "service-clearing", "clearing-v2 unmapped"
assert m.get("com.chorus.go") == "service-werk", "go unmapped"
assert "com.chorus.nightly-suites" not in m, "retired nightly-suites still mapped"
EOF
}

# land-4980-rows runs the sensor once after the land (the prove runs before the next hourly
# run) and refuses unless it ends green; the runner is stubbed here
land() { # <stub stdout> <stub exit>
  printf '#!/bin/sh\necho "$*" > "%s/call"\nprintf "%%s\\n" "%s"\nexit %s\n' "$BATS_TEST_TMPDIR" "$1" "$2" > "$BATS_TEST_TMPDIR/runner"
  chmod +x "$BATS_TEST_TMPDIR/runner"
  printf '#!/bin/sh\necho census >> "%s/call"\necho "shacl-census: 6755 violation(s)"\nexit "${CENSUS_RC:-0}"\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/census"
  chmod +x "$BATS_TEST_TMPDIR/census"
  CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" SHACL_CENSUS="$BATS_TEST_TMPDIR/census" python3 "$ROOT/platform/scripts/land-4980-rows" --apply
}

@test "the land runs the services sensor through the runner and passes on green" {
  run land "services: green — every unit is mapped or external, nothing stale" 0
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(head -1 "$BATS_TEST_TMPDIR/call")" = "call herald-run sensor=services wait=true" ] || return 1
  [ "$(sed -n 2p "$BATS_TEST_TMPDIR/call")" = census ] || { cat "$BATS_TEST_TMPDIR/call"; return 1; }
}

# NEGATIVE PROOF: a red run, an unmeasured one, or a runner that fails stops the land
@test "a red or unmeasured sensor run, or a failed runner, refuses the land" {
  run land "services: RED — 3 finding(s)" 1
  [ "$status" -eq 1 ] && [[ "$output" == *"did not end green"* ]] || { echo "$output"; return 1; }
  run land "sensor services: run x red because: unmeasured" 0
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  run land "services: green" 2
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: a census that fails stops the land; a red sensor never reaches the census
@test "a failed census refuses the land, and a red sensor runs no census" {
  CENSUS_RC=1 run land "services: green" 0
  [ "$status" -eq 1 ] && [[ "$output" == *"SHACL census failed"* ]] || { echo "$output"; return 1; }
  run land "services: RED — 3 finding(s)" 1
  ! grep -q census "$BATS_TEST_TMPDIR/call" || return 1
}

@test "a dry run runs nothing" {
  CHORUS_RUNNER=/nonexistent run python3 "$ROOT/platform/scripts/land-4980-rows"
  [ "$status" -eq 0 ] && [[ "$output" == plan:* ]] || { echo "$output"; return 1; }
}
