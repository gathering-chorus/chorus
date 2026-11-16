#!/usr/bin/env bats
# @test-type: e2e — after the land: reads the live Loki for the messages sensor's newest run and
# the current Clearing's bridge room; writes nothing
# @domain: messages
# @card: #4984
#
# AC4 (after test): the messages sensor has a measured reading for the day and the bridge room
# has no 404 at the relay dial for an hour. Before #4984: unmeasured every day 10-26..11-16
# ("relay sent no EOSE within 60000ms"), and 404 at the dial every 5 s since 10-25 22:01.

setup() {
  LOKI="${LOKI_URL:-http://localhost:3102}"
}

loki() {  # $1 query, $2 seconds back
  curl -sf --max-time 20 -G "$LOKI/loki/api/v1/query_range" --data-urlencode "query=$1" \
    --data-urlencode "start=$(( $(date +%s) - $2 ))000000000" --data-urlencode "end=$(date +%s)000000000" \
    --data-urlencode limit=5000
}

# verdicts over Loki answers: sensor($1 json) and bridge($1 json); 0 pass, 1 fail, 2 unmeasured
sensor_verdict() {
  python3 -c '
import json, sys
d = json.loads(sys.argv[1]); v = sorted(x for r in d["data"]["result"] for x in r["values"])
if not v: print("UNMEASURED no messages sensor run in the last day"); sys.exit(2)
line = v[-1][1]
print(line)
sys.exit(1 if " unmeasured at " in line else 0)' "$1"
}

bridge_verdict() {
  python3 -c '
import json, sys
d = json.loads(sys.argv[1])
ev = [json.loads(x[1]) for r in d["data"]["result"] for x in r["values"]]
n404 = sum(1 for e in ev if e.get("event") == "buzz.room.socket_error" and "404" in str(e.get("reason", "")))
print(f"404 at the dial in the last hour: {n404}")
sys.exit(1 if n404 else 0)' "$1"
}

@test "the messages sensor's newest run measured messages (not unmeasured)" {
  case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4984 AC4)";; esac
  q='{job="daemon-logs", filename=~".*memory-sensors.log"} |~ "sensor messages: run .* (green|red|unmeasured) at"'
  run sensor_verdict "$(loki "$q" 86400)"
  echo "$output"
  [ "$status" -ne 2 ] || skip "$output"
  [ "$status" -eq 0 ]
}

@test "the bridge room had no 404 at the relay dial for an hour" {
  case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4984 AC4)";; esac
  run bridge_verdict "$(loki '{job="chorus-bridge"} |= "buzz.room."' 3600)"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: an unmeasured newest run, or a 404 in the hour, fails" {
  run sensor_verdict '{"data":{"result":[{"values":[["1","sensor messages: run m-1 unmeasured at 2026-11-16T09:46:05Z"]]}]}}'
  [ "$status" -eq 1 ]
  run bridge_verdict '{"data":{"result":[{"values":[["1","{\"event\":\"buzz.room.socket_error\",\"reason\":\"Unexpected server response: 404\"}"]]}]}}'
  [ "$status" -eq 1 ]
}
