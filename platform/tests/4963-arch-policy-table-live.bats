#!/usr/bin/env bats
# @test-type: smoke — reads prod chorus-api over the LAN name; read-only.
# @domain: policies
# @needs: machine — prod after the land
# @card: #4963
# @owner: silas
#
# #4963 AC4, live — after the land, the page and its route answer on the machine's LAN name
# (what Jeff's phone opens): every arch policy measured, worst first, the 13 of this card among
# them (the gate also holds #4960's arch checks). A policy it could not measure is red.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/domains/policies/policy-checks-4963.json"
BASE="${POLICY_COUNTS_BASE:-http://jeffs-mac-mini-m1-3.local:3340}"

@test "the page answers on the LAN name" {
  run curl -sf --max-time 10 -o /dev/null -w '%{http_code}' "$BASE/loom/policy-counts.html"
  [ "$output" = "200" ] || { echo "$output"; return 1; }
}

@test "the route gives every arch policy measured, worst first, with the 13 of this card" {
  run sh -c "curl -sf --max-time 320 '$BASE/api/chorus/policies/counts?gate=arch'"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | python3 -c '
import json, sys
p = json.load(sys.stdin)["policies"]
want = {r["name"] for r in json.load(open(sys.argv[1]))}
assert want <= {x["name"] for x in p}, want - {x["name"] for x in p}
assert all(x["verdict"] in ("pass", "fail") and x["count"] is not None for x in p), [x for x in p if x["count"] is None]
c = [x["count"] for x in p if x["verdict"] == "pass"]
assert c == sorted(c, reverse=True), c
assert sum(x["narrowed"] for x in p) >= 10, "narrowed rules not marked"
' "$CHECKS" || return 1
}
