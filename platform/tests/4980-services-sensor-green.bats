#!/usr/bin/env bats
# @test-type: integration — reads Loki (sensor.run.ended) for the live case; fixtures and the map file for the rest
# @domain: services
# @card: #4980 · owner: silas
# Checks platform/config/service-instance-map.json and the services sensor's runs.
#
# #4980 AC1 — the services sensor ran red every hour on 2026-11-15 (29 findings, from
# /tmp/service-harvest.log): 26 dead ServiceInstance rows (22 leaked quartet-test probes, 4
# gone units; deleted 09:05 on Jeff's word), 2 running units the map did not name
# (com.chorus.clearing-v2, com.chorus.go) and 1 mapping for a retired unit
# (com.chorus.nightly-suites, #4562). The map names the two and drops the one; after the
# land the sensor's newest scheduled run ends green.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MAP="$ROOT/platform/config/service-instance-map.json"
}

# Loki query_range JSON on stdin: the newest services run's outcome
newest() {
  python3 -c '
import json, sys
best = None
for s in json.load(sys.stdin).get("data", {}).get("result", []):
    for ts, line in s.get("values", []):
        try: e = json.loads(line)
        except ValueError: continue
        if e.get("event") == "sensor.run.ended" and e.get("sensor") == "services":
            if best is None or int(ts) > best[0]: best = (int(ts), e.get("outcome"), e.get("reason", ""))
if best is None: print("NO RUN"); sys.exit(1)
print(best[1], best[2]); sys.exit(0 if best[1] == "green" else 1)
'
}

runs() { # lines of: <ts> <outcome>
  local vals="" sep=""
  while read -r ts o; do
    [ -n "$ts" ] || continue
    vals="$vals$sep[\"$ts\",$(printf '{"event":"sensor.run.ended","sensor":"services","outcome":"%s"}' "$o" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')]"; sep=","
  done
  printf '{"data":{"result":[{"values":[%s]}]}}' "$vals"
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

@test "a newest green run reads green" {
  run bash -c "$(declare -f newest runs); printf '1 red\n2 green\n' | runs | newest"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: a red after a green fails; no run at all fails (unmeasured is not green)
@test "a newest red run, or no run, fails" {
  run bash -c "$(declare -f newest runs); printf '1 green\n2 red\n' | runs | newest"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  run bash -c "$(declare -f newest runs); printf '' | runs | newest"
  [ "$status" -eq 1 ] && [[ "$output" == "NO RUN"* ]] || { echo "$output"; return 1; }
}

# After the land: the sensor's newest scheduled run is green. Before it (this tree is not on
# main yet) the map change has not reached prod, so there is nothing to read.
@test "after the land: the services sensor's newest run ends green" {
  # landed = main carries a #4980 commit and this tree is main's (a card werk has commits main lacks)
  { [ -n "$(git -C "$ROOT" log origin/main --grep='#4980' --format=%h -1 2>/dev/null)" ] \
    && git -C "$ROOT" merge-base --is-ancestor HEAD origin/main 2>/dev/null; } \
    || skip "before the land: #4980 is not on main yet (AC1)"
  json="$(curl -sf -m 20 -G "${LOKI_URL:-http://localhost:3102}/loki/api/v1/query_range" \
    --data-urlencode 'query={job="platform-chorus"} |= "sensor.run.ended" |= "\"sensor\":\"services\""' \
    --data-urlencode "since=3h" --data-urlencode "limit=500")" || { echo "Loki unreadable — unmeasured"; return 1; }
  run newest <<<"$json"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
