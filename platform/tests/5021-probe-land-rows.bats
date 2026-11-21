#!/usr/bin/env bats
# @test-type: unit — land-5021-rows against a fixture door (LAND_LIVE_JSON); no store, no runner
# @domain: identity
# @card: #5021 (folded from 5026) · owner: kade
# Covers: platform/scripts/land-5021-rows
#
# The login probe gets its own Role and Teammate, zero grants, so its sign-in can name the role
# it acts as (AC1) and its next sign-in closes its own expired rows (#4966).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  S="$ROOT/platform/scripts/land-5021-rows"
  T="$BATS_TEST_TMPDIR"
}

# the door as served, plus card 5021's policy rows already there, so only the probe rows differ
live() {
  printf '%s' "$1" > "$T/probe.json"
  python3 -c '
import json, sys
rows = json.load(open(sys.argv[1] + "/designing/domains/policies/policy-checks-5021.json"))
live = json.load(open(sys.argv[2]))
live["policies"] = [{k: r[k] for k in ("name", "label", "comment", "gate", "status", "baseline", "checkKind", "checkSpec", "violationFixture")} for r in rows]
json.dump(live, open(sys.argv[3], "w"))' "$ROOT" "$T/probe.json" "$T/live.json"
  LAND_LIVE_JSON="$T/live.json" run python3 "$S"
}

@test "dry run on a door without the probe's rows: 2 adds, roles before teammates, nothing written" {
  live '{"roles":[],"teammates":[]}'
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | grep -c '^ADD')" -eq 2 ]
  [ "$(printf '%s\n' "$output" | grep -n '^ADD' | head -1 | grep -c 'roles/flow-probe')" -eq 1 ]
  [[ "$output" == *"2 add, 0 change, 0 delete"*"nothing written (dry run)"* ]] || return 1
}

@test "re-run once the door serves them (prefixed links): 0 to write" {
  live '{"roles":[{"name":"flow-probe","label":"flow-probe (browser-flow test identity)","comment":"The login probe'"'"'s role: walks Jeff'"'"'s browser path. No grants (#5026).","roleKind":"automation"}],
         "teammates":[{"name":"flow-probe","label":"flow-probe","servesAs":"chorus:role-flow-probe","signsInAs":"chorus:principal-flow-probe"}]}'
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 add, 0 change, 0 delete;"* ]] || return 1
}

# NEGATIVE PROOF: a probe Teammate serving some other role is a change, never "same"
@test "NEGATIVE: a probe teammate that serves another role is caught as a change" {
  live '{"roles":[],"teammates":[{"name":"flow-probe","label":"flow-probe","servesAs":"chorus:role-wren","signsInAs":"chorus:principal-flow-probe"}]}'
  [ "$status" -eq 0 ]
  [[ "$output" == *"CHANGE teammates/flow-probe"*'"servesAs": "flow-probe"'* ]] || return 1
}

@test "zero grants: the plan writes no Permission row, only policies and the probe's two classes" {
  run python3 -c "
import importlib.machinery, importlib.util
l = importlib.machinery.SourceFileLoader('l', '$S'); s = importlib.util.spec_from_loader('l', l); m = importlib.util.module_from_spec(s); l.exec_module(m)
print(sorted({c for c, _, _ in m.plan({})}))"
  [ "$output" = "['policies', 'roles', 'teammates']" ]
}
