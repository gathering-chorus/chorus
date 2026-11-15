#!/usr/bin/env bats
# @test-type: unit — runs a throwaway Loki (its own temp dir and ports), pushes made-up events, evaluates each row-made rule's exact LogQL; never the team's Loki
# @domain: monitors
# @card: #4977 · owner: silas
# Checks platform/scripts/monitor-alert and designing/domains/monitors/{sensor-runs,buzz-shadow}-*-4977.json.
#
# #4977 AC4 — the two Loki-read monitors, both ways, on Loki's own engine (promtool
# cannot run LogQL): no red sensor run → nothing; a red run → one result naming the
# sensor and its owner; 75 minutes later → cleared. Buzz shadow: a matching delivery
# → nothing; a mismatch → fires; it stays silent to people while PULSE_BUZZ_DELIVER
# is off because its Alert row says notify off (4970-off-alerts-silent.bats).

# the same Loki build the team's Loki runs: found from that process, or LOKI_BIN, or PATH
LOKI_BIN="${LOKI_BIN:-$(ps -axo comm= | grep -E '/loki$' | head -1)}"
LOKI_BIN="${LOKI_BIN:-$(command -v loki)}"

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export ROOT T="$(mktemp -d)"
  [ -x "$LOKI_BIN" ] || { echo "no loki binary at $LOKI_BIN" >&2; return 1; }
  export PORT=$(( 41000 + $$ % 1000 )) GPORT=$(( 42000 + $$ % 1000 ))
  cat > "$T/loki.yaml" <<YAML
auth_enabled: false
server: { http_listen_port: $PORT, grpc_listen_port: $GPORT, log_level: error }
common:
  instance_addr: 127.0.0.1
  path_prefix: $T/data
  storage: { filesystem: { chunks_directory: $T/data/chunks, rules_directory: $T/data/rules } }
  replication_factor: 1
  ring: { kvstore: { store: inmemory } }
schema_config:
  configs: [ { from: 2020-10-24, store: boltdb-shipper, object_store: filesystem, schema: v11, index: { prefix: index_, period: 24h } } ]
limits_config: { reject_old_samples: false }
ingester: { max_chunk_age: 8784h }
YAML
  "$LOKI_BIN" -config.file="$T/loki.yaml" > "$T/loki.log" 2>&1 &
  echo $! > "$T/pid"
  for _ in $(seq 1 60); do curl -sf "http://127.0.0.1:$PORT/ready" >/dev/null 2>&1 && break; sleep 0.5; done
  curl -sf "http://127.0.0.1:$PORT/ready" >/dev/null || { cat "$T/loki.log" >&2; return 1; }
  # one red sensor run at T0, one matching and one mismatching Buzz check at T0
  export T0=$(( $(date +%s) - 3 * 3600 ))
  push() { curl -sf -H 'Content-Type: application/json' "http://127.0.0.1:$PORT/loki/api/v1/push" --data-binary "$1" >/dev/null; }
  push "{\"streams\":[{\"stream\":{\"job\":\"platform-chorus\",\"appName\":\"chorus-events\"},\"values\":[
    [\"$((T0))000000000\",\"{\\\"event\\\":\\\"sensor.run.ended\\\",\\\"sensor\\\":\\\"services\\\",\\\"owner\\\":\\\"silas\\\",\\\"outcome\\\":\\\"red\\\"}\"],
    [\"$((T0 - 600))000000000\",\"{\\\"event\\\":\\\"sensor.run.ended\\\",\\\"sensor\\\":\\\"files\\\",\\\"owner\\\":\\\"kade\\\",\\\"outcome\\\":\\\"green\\\"}\"]]},
    {\"stream\":{\"job\":\"platform-chorus\"},\"values\":[
    [\"$((T0 - 1800))000000000\",\"{\\\"event\\\":\\\"buzz.delivery.shadow\\\",\\\"verdict\\\":\\\"match\\\"}\"],
    [\"$((T0))000000000\",\"{\\\"event\\\":\\\"buzz.delivery.shadow\\\",\\\"verdict\\\":\\\"mismatch\\\"}\"]]}]}" || return 1
  sleep 2
}
teardown_file() { [ -f "$T/pid" ] && kill "$(cat "$T/pid")" 2>/dev/null; rm -rf "$T"; }

# the made rule's query, evaluated by Loki at unix second $2; prints one line per result: labels value
q() {
  local expr; expr=$("$ROOT/platform/scripts/monitor-alert" build "$ROOT/designing/domains/monitors/$1-monitor-4977.json" "$ROOT/designing/domains/monitors/$3-sensor-4977.json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["metricQuery"])')
  curl -sf -G "http://127.0.0.1:$PORT/loki/api/v1/query" --data-urlencode "query=$expr" --data-urlencode "time=$2" \
    | python3 -c 'import json,sys;[print(json.dumps(r["metric"],sort_keys=True), r["value"][1]) for r in json.load(sys.stdin)["data"]["result"]]'
}

@test "sensor-run: before any red run, nothing" {
  run q sensor-run-not-green $(( T0 - 300 )) sensor-runs
  [ "$status" -eq 0 ] || return 1
  [ -z "$output" ] || return 1
}
@test "sensor-run: a red run fires once, naming the sensor and its owner; a green run never does" {
  run q sensor-run-not-green $(( T0 + 60 )) sensor-runs
  [ "$status" -eq 0 ] || return 1
  [ "$output" = '{"owner": "silas", "sensor": "services"} 1' ] || return 1
}
@test "sensor-run: 75 minutes after the red run, cleared" {
  run q sensor-run-not-green $(( T0 + 76 * 60 )) sensor-runs
  [ -z "$output" ] || return 1
}
@test "Buzz: a matching delivery alone sends nothing" {
  run q buzz-shadow $(( T0 - 300 )) buzz-shadow
  [ "$status" -eq 0 ] || return 1
  [ -z "$output" ] || return 1
}
@test "Buzz: a mismatch fires; 15 minutes on, cleared" {
  run q buzz-shadow $(( T0 + 60 )) buzz-shadow
  [[ "$output" == *" 1" ]] || return 1
  run q buzz-shadow $(( T0 + 16 * 60 )) buzz-shadow
  [ -z "$output" ] || return 1
}
