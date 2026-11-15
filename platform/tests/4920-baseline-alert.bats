#!/usr/bin/env bats
# @test-type: integration — a scratch Fuseki (lib/scratch-census) for the census; reads the live Grafana rule, writes nothing to it
# @domain: persistence
# @card: #4920 · owner: silas
# wipe-guard: exempt — every write goes to the scratch Fuseki lib/scratch-census boots on a free port, never :3030
#
# #4920 AC3: the baseline equals the named remainder and only goes down, and the alert fires on any
# violation above it — a seeded bad row reads red. The rule judged here is the one Grafana holds now
# (shacl-above-baseline), applied to the gauges the census writes.

load lib/scratch-census

RULE_URL="${GRAFANA_URL:-http://localhost:3100}/api/prometheus/grafana/api/v1/rules?rule_uid=shacl-above-baseline"
QUERY='(max(athena_shacl_violations) - max(athena_shacl_violations_baseline)) > 0'

# the live rule's query, from Grafana itself
live_query() {
  curl -sf --max-time 10 "$RULE_URL" | python3 -c '
import json, sys
for g in json.load(sys.stdin)["data"]["groups"]:
    for r in g["rules"]:
        if r.get("name") == "SHACL violations above baseline": print(r["query"])'
}

# the rule, applied to a census textfile: "red" when it would fire, "green" when not
reads() {
  local v b; v=$(gauge athena_shacl_violations "$1"); b=$(gauge athena_shacl_violations_baseline "$1")
  [ -n "$v" ] && [ -n "$b" ] || { echo "no gauges in $1"; return 1; }
  if [ $((v - b)) -gt 0 ]; then echo red; else echo green; fi
}

@test "the live alert rule is the one judged here" {
  q="$(live_query)"
  [ -n "$q" ] || { echo "Grafana does not serve shacl-above-baseline at $RULE_URL"; return 1; }
  [ "$q" = "$QUERY" ] || { echo "live query: $q"; return 1; }
}

@test "at the baseline the rule reads green; a seeded bad row reads red; the fix reads green and lowers the baseline" {
  f="$TEXTFILE_DIR/athena_shacl.prom"
  alerts 4 1
  run "$ROOT/platform/scripts/shacl-census"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(reads "$f")" = green ] || { cat "$f"; return 1; }
  # NEGATIVE PROOF: one seeded bad row above the baseline fires the rule
  alerts 4 2
  run "$ROOT/platform/scripts/shacl-census"
  [ "$(reads "$f")" = red ] || { cat "$f"; return 1; }
  [ "$(gauge athena_shacl_violations_baseline "$f")" = 2 ] || { cat "$f"; return 1; }
  # the fix: below the old baseline, the baseline follows it down and the rule is green
  alerts 4 0
  run "$ROOT/platform/scripts/shacl-census"
  [ "$(reads "$f")" = green ] && [ "$(gauge athena_shacl_violations_baseline "$f")" = 0 ] || { cat "$f"; return 1; }
}
