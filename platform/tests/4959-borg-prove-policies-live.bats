#!/usr/bin/env bats
# @test-type: smoke — runs the canonical policy-validate --runtime against prod; read-only.
# @domain: policies
# @needs: machine — prod after the land
# @card: #4959
# @owner: silas
#
# #4959 AC6 — after the land, every runtime policy is at or below its baseline in prod.
# This is also the check a child card filed by borg-prove carries: it goes green when
# the policy it names is back at its baseline. A run that could not measure is red,
# never green (#3734). The pure cases (one child per red policy, unmeasured on a
# broken run) are borg-prove's tests/policy_runtime_4959.rs and land_gate_4683.rs.

PV="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/platform/scripts/policy-validate"

@test "the canonical checker is on main" {
  [ -x "$PV" ] || { echo "no $PV"; return 1; }
}

@test "every runtime policy is at or below its baseline in prod" {
  run "$PV" --runtime
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"0 could not run"* ]] || return 1
}
