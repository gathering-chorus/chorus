#!/usr/bin/env bats
# @test-type: unit — evaluates each row-made rule's exact query with promtool over made-up series; no Grafana, no network
# @domain: monitors
# @card: #4977 · owner: silas
# Checks platform/scripts/monitor-alert and the rows in designing/domains/monitors/*-4977.json.
#
# #4977 AC4 — negative proof both ways for each Prometheus-read monitor: under its
# line sends nothing, over its line fires naming what is wrong, recovery clears it.
# A second evaluation while still over sends nothing because every made rule tells
# once (repeatInterval 8760h, checked below). The crawler cases are the five that
# 4647-crawler-out-of-sync.bats held for the retired yml (Kade, 2026-11-14 21:0x).
# The rule's `for` is modelled with promtool's own `for`, from the row.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export ROOT TMPD="$(mktemp -d)"
  command -v promtool >/dev/null || { echo "promtool not on PATH" >&2; return 1; }
}
teardown_file() { rm -rf "$TMPD"; }

M() { echo "$ROOT/designing/domains/monitors/$1-4977.json"; }
# build the Alert row for monitor $1 / sensor $2 into $TMPD/$1.json
build() { "$ROOT/platform/scripts/monitor-alert" build "$(M "$1")" "$(M "$2")" > "$TMPD/$1.json"; }
# promtool test: $1 alert name, $2 python list of input_series, $3 python list of alert_rule_test
ptest() {
  python3 - "$TMPD/$1.json" "$2" "$3" "$TMPD/$1" <<'PY'
import json, sys, os
row = json.load(open(sys.argv[1])); d = sys.argv[4]; os.makedirs(d, exist_ok=True)
f = row["forDuration"]
json.dump({"groups": [{"name": "g", "rules": [{"alert": row["name"], "expr": row["metricQuery"], "for": f}]}]}, open(f"{d}/rules.yml", "w"))
json.dump({"rule_files": ["rules.yml"], "evaluation_interval": "1m",
           "tests": [{"interval": "1m", "input_series": eval(sys.argv[2]), "alert_rule_test": eval(sys.argv[3])}]}, open(f"{d}/test.yml", "w"))
PY
  (cd "$TMPD/$1" && promtool test rules test.yml)
}
S() { echo "{'series': '$1', 'values': '$2'}"; }
A() { echo "{'eval_time': '$1', 'alertname': '$2', 'exp_alerts': [$3]}"; }

SYNC='chorus_sensor_reading{sensor="sync",metric="missing",subject="codefile",machine="library"}'
STALE='chorus_sensor_reading{sensor="sync",metric="stale",subject="codefile",machine="library"}'
AGE='node_textfile_mtime_seconds{file="/x/sensor_sync.prom",machine="library"}'
# expected labels, kept in single quotes so bash never brace-expands their commas
L_SYNC='{"exp_labels": {"metric": "missing"}}'
L_AGE='{"exp_labels": {"file": "/x/sensor_sync.prom", "machine": "library"}}'
L_JAVA='{"exp_labels": {"name": "java", "machine": "library"}}'
L_NONE='{"exp_labels": {}}'
L_ABSENT='{"exp_labels": {"sensor": "sync"}}'

@test "every made rule tells once and carries its row's wait" {
  for p in "crawler-out-of-sync crawler-sync-sensor" "crawler-sync-quiet crawler-sync-age-sensor" "fuseki-memory fuseki-memory-sensor" "shacl-above-baseline shacl-above-baseline-sensor" "sensor-run-not-green sensor-runs-sensor" "buzz-shadow buzz-shadow-sensor"; do
    set -- $p; build "$1-monitor" "$2" || return 1
    python3 -c "import json,sys;r=json.load(open(sys.argv[1]));assert r['repeatInterval']=='8760h' and r['forDuration'], r" "$TMPD/$1-monitor.json" || return 1
  done
}

# --- crawler A: out of sync (the five 4647 cases, 1-3 here) ---
@test "crawler 1: rows out of sync for over an hour fire, naming the metric (the gauges carry no class; the text points at /sync)" {
  build crawler-out-of-sync-monitor crawler-sync-sensor
  ptest crawler-out-of-sync-monitor "[$(S "$SYNC" '3x90'), $(S "$STALE" '0x90')]" \
    "[$(A 30m crawler-out-of-sync ''), $(A 61m crawler-out-of-sync "$L_SYNC")]"
}
@test "crawler 2: control, every class at zero is ok" {
  build crawler-out-of-sync-monitor crawler-sync-sensor
  ptest crawler-out-of-sync-monitor "[$(S "$SYNC" '0x90'), $(S "$STALE" '0x90')]" "[$(A 61m crawler-out-of-sync ''), $(A 89m crawler-out-of-sync '')]"
}
@test "crawler 3: a gap under an hour waits, and a gap that closes clears" {
  build crawler-out-of-sync-monitor crawler-sync-sensor
  ptest crawler-out-of-sync-monitor "[$(S "$SYNC" '2x40 0x50'), $(S "$STALE" '0x90')]" "[$(A 39m crawler-out-of-sync ''), $(A 80m crawler-out-of-sync '')]"
}
# --- crawler B: the sensor gone quiet (cases 4-5) ---
@test "crawler 4, NEGATIVE PROOF: no sync reading at all is a finding, never ok (absent(), not noDataState)" {
  build crawler-sync-quiet-monitor crawler-sync-age-sensor
  # the gauge file is there, but the sync sensor wrote no reading
  ptest crawler-sync-quiet-monitor "[$(S "$AGE" '0x5')]" "[$(A 2m crawler-sync-quiet "$L_ABSENT")]"
}
@test "crawler 5, NEGATIVE PROOF: quiet for over 26 hours is stale even at zero; fresh is ok" {
  build crawler-sync-quiet-monitor crawler-sync-age-sensor
  # promtool's clock starts at 0: an mtime of 0 is fresh at minute 0 and 26h+ old after 1561 minutes
  ptest crawler-sync-quiet-monitor "[$(S "$SYNC" '0x1600'), $(S "$AGE" '0x1600')]" \
    "[$(A 60m crawler-sync-quiet ''), $(A 1561m crawler-sync-quiet "$L_AGE")]"
}
# --- Fuseki and SHACL ---
@test "Fuseki: 2620 MB is quiet, 3600 MB for 5m fires naming the machine, back under clears" {
  build fuseki-memory-monitor fuseki-memory-sensor
  F='host_process_resident_memory_bytes{name="java",machine="library"}'
  ptest fuseki-memory-monitor "[$(S "$F" '2747269120x10 3774873600x10 2747269120x10')]" \
    "[$(A 9m fuseki-memory-high ''), $(A 17m fuseki-memory-high "$L_JAVA"), $(A 25m fuseki-memory-high '')]"
}
@test "SHACL: at the baseline is quiet, above it for 5m fires, back to the baseline clears" {
  build shacl-above-baseline-monitor shacl-above-baseline-sensor
  ptest shacl-above-baseline-monitor "[$(S 'athena_shacl_violations' '9892x10 9900x10 9892x10'), $(S 'athena_shacl_violations_baseline' '9892x30')]" \
    "[$(A 9m shacl-above-baseline ''), $(A 17m shacl-above-baseline "$L_NONE"), $(A 25m shacl-above-baseline '')]"
}
# --- the two Loki-read monitors: promtool cannot run LogQL, so they run on a throwaway Loki ---
@test "sensor-run-not-green and Buzz shadow, both ways, on Loki's own engine (4977-loki-rules-fire-once-clear.bats)" {
  run bats "$ROOT/platform/tests/4977-loki-rules-fire-once-clear.bats"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"ok 5 "* ]] || return 1
}
