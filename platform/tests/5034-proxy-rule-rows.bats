#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-5034-proxy-rows against a fixture door (LAND_LIVE_JSON); no store, no runner
# @domain: properties
# @card: #5034 · owner: silas (rows), wren (card)
#
# The /owl proxy forwards to a generated API only when two rule rows allow its address, and
# forwards nothing when either is missing. Silas wrote the rows by hand at 15:15 on 11-24; the
# code review asked what recreates them. platform/scripts/land-5034-proxy-rows does: the two
# registered keys first, then the two rules. This holds what it plans, offline:
#   - an empty store        → four adds, the keys before the rules that use them;
#   - all four present      → nothing to write;
#   - NEGATIVE: a rule row holding a wider value (0.0.0.0) is a change back to 127.0.0.1,
#     so the compare can tell the rule from a loosened one; a dry run never calls the runner.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/scripts/land-5034-proxy-rows"

setup() {
  T="$(mktemp -d)"
  printf '#!/bin/bash\necho called >> "%s/runner.log"\n' "$T" > "$T/runner"
  chmod +x "$T/runner"
}
teardown() { rm -rf "$T"; }

# the four rows as the door serves them; $1 = the allowed-hosts value
live_with_hosts() {
  cat > "$T/live.json" <<JSON
{"keys":[
 {"name":"pk-owlproxyallowedhosts","label":"pk-owlProxyAllowedHosts","keyName":"owl.proxy.allowed_hosts","appliesToClass":"https://jeffbridwell.com/chorus#Service","propertyKind":"datatype","lifecycleEnabled":"false","comment":"#5034: hosts the chorus-api /owl proxy may forward to (comma list); read at proxy-map build; absent = nothing forwarded"},
 {"name":"pk-owlproxyportrange","label":"pk-owlProxyPortRange","keyName":"owl.proxy.port_range","appliesToClass":"https://jeffbridwell.com/chorus#Service","propertyKind":"datatype","lifecycleEnabled":"false","comment":"#5034: port range lo-hi the chorus-api /owl proxy may forward to; absent = nothing forwarded"}],
"properties":[
 {"name":"prop-owl-proxy-allowed-hosts","label":"prop-owl-proxy-allowed-hosts","propertyKey":"owl.proxy.allowed_hosts","propertyValue":"$1","propertyValueType":"string","propertyScope":"global","comment":"#5034: hosts the chorus-api /owl proxy may forward to, read from each generated door server.bind row; comma list; a row whose host is not listed is skipped and logged; missing row = nothing forwarded"},
 {"name":"prop-owl-proxy-port-range","label":"prop-owl-proxy-port-range","propertyKey":"owl.proxy.port_range","propertyValue":"1024-65535","propertyValueType":"string","propertyScope":"global","comment":"#5034: port range (lo-hi) the chorus-api /owl proxy may forward to; chorus-api own port is always refused in code; missing row = nothing forwarded"}]}
JSON
}

plan() { LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"; }

@test "#5034 an empty store: four adds, each key before the rule that uses it" {
  echo '{"keys":[],"properties":[]}' > "$T/live.json"
  plan
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '4 add, 0 change, 0 delete; 0 same' || { echo "$output"; return 1; }
  first_rule="$(echo "$output" | grep -n '^ADD *properties/' | head -1 | cut -d: -f1)"
  last_key="$(echo "$output" | grep -n '^ADD *keys/' | tail -1 | cut -d: -f1)"
  test -n "$first_rule" && test -n "$last_key" && test "$last_key" -lt "$first_rule" || { echo "keys are not planned before rules: $output"; return 1; }
}

@test "#5034 all four rows present: nothing to write" {
  live_with_hosts '127.0.0.1'
  plan
  echo "$output" | grep -q '0 add, 0 change, 0 delete; 4 same' || { echo "$output"; return 1; }
}

@test "#5034 NEGATIVE: a loosened host rule is a change back, and a dry run never calls the runner" {
  live_with_hosts '0.0.0.0'
  plan
  echo "$output" | grep -q '0 add, 1 change, 0 delete; 3 same' || { echo "a loosened rule read as same: $output"; return 1; }
  echo "$output" | grep -q 'CHANGE properties/prop-owl-proxy-allowed-hosts' || { echo "$output"; return 1; }
  test ! -e "$T/runner.log" || { echo "the dry run called the runner"; return 1; }
}
