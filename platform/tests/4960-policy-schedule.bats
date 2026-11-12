#!/usr/bin/env bats
# @test-type: unit — chorus-make drift, the sensor script against fixture policies, promtool over the stale rule; no store, no dagu run.
# @domain: policies
# @card: #4960
# @owner: silas
#
# #4960 AC3 — every policy check runs hourly against main and its counts reach Pulse:
# platform/pipelines/policy-sensor.yaml is what chorus-make generates from its rows (the
# same rows land-4960-rows writes to the store), its queue runs one at a time, the sensor
# writes the textfile, and the stale rule fires when the file ages past 3 h or never
# appeared — never silent (#3734). Negative proofs: a hand-edited pipeline is drift; a
# sensor that cannot measure writes nothing; an old or absent file alerts.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
F="$ROOT/platform/services/chorus-make/tests/fixtures"
YAML="$ROOT/platform/pipelines/policy-sensor.yaml"
RULES="$ROOT/platform/config/alerting/chorus-policy-rules.yaml"

setup() {
  T="$BATS_TEST_TMPDIR"
}

cm() {
  CARGO_TARGET_DIR="$ROOT/platform/services/chorus-make/target/$(id -un)" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null 2>&1 || return 2
  "$ROOT/platform/services/chorus-make/target/$(id -un)/debug/chorus-make" "$@"
}

@test "the pipeline file is what chorus-make generates from its rows: hourly, its own queue, the sensor" {
  run cm check-straight policy-sensor "$F/policy-sensor-rows.tsv" "$F/policy-sensor-pipe.tsv" "" "$YAML"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q '^schedule: "7 \* \* \* \*"$' "$YAML" || return 1
  grep -q '^queue: policy-sensor$' "$YAML" || return 1
  grep -q 'command: "${CHORUS_HOME}/platform/scripts/policy-sensor"' "$YAML" || return 1
  grep -A1 -- '- name: policy-sensor' "$ROOT/platform/config/dagu.yaml" | grep -q 'max_concurrency: 1' || return 1
}

@test "negative proof: a hand-edited pipeline file is drift" {
  sed 's/7 \* \* \* \*/0 3 * * */' "$YAML" > "$T/edited.yaml"
  run cm check-straight policy-sensor "$F/policy-sensor-rows.tsv" "$F/policy-sensor-pipe.tsv" "" "$T/edited.yaml"
  [ "$status" -ne 0 ] || return 1
}

@test "the store gets the same pipeline: land-4960-rows plans its four rows from the same fixtures" {
  printf '{}' > "$T/live.json"
  LAND_LIVE_JSON="$T/live.json" run python3 "$ROOT/platform/scripts/land-4960-rows"
  for row in "skills/policy-sensor" "pipelinesteps/policy-sensor-measure" "stepskills/policy-sensor-measure-1" "pipelines/policy-sensor"; do
    [[ "$output" == *"ADD    $row "* ]] || { echo "no $row"; return 1; }
  done
  run python3 -c "import runpy; rows = runpy.run_path('$ROOT/platform/scripts/land-4960-rows')['sensor_rows'](); p = [w for c, n, w in rows if c == 'pipelines'][0]; print(p['runSchedule'] + '|' + p['runQueue'])"
  [ "$output" = "7 * * * *|policy-sensor" ] || { echo "$output"; return 1; }
}

@test "the sensor runs in canonical main and writes the counts" {
  home="$T/home"; mkdir -p "$home/platform/scripts"
  cp "$ROOT/platform/scripts/policy-validate" "$home/platform/scripts/"
  cp "$ROOT/platform/scripts/policy-sensor" "$home/platform/scripts/"
  (cd "$home" && git init -q . && touch run.log && git add -A)
  printf '{"data":[{"name":"no-log-files","gate":"quality","status":"active","baseline":"0","checkKind":"command","checkSpec":"git ls-files *.log"}]}' > "$T/p.json"
  cd "$T"
  CHORUS_HOME="$home" POLICIES_JSON="$T/p.json" TEXTFILE_DIR="$T/tf" run "$home/platform/scripts/policy-sensor"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }   # one log file in main: over its baseline
  grep -q '^chorus_policy_violations{policy="no-log-files",gate="quality"} 1$' "$T/tf/chorus_policy.prom" || return 1
}

@test "negative proof: a sensor that cannot measure writes nothing" {
  home="$T/home2"; mkdir -p "$home/platform/scripts"
  cp "$ROOT/platform/scripts/policy-validate" "$ROOT/platform/scripts/policy-sensor" "$home/platform/scripts/"
  CHORUS_HOME="$home" POLICIES_JSON="$T/missing.json" TEXTFILE_DIR="$T/tf2" run "$home/platform/scripts/policy-sensor"
  [ "$status" -eq 2 ] || return 1
  [ ! -e "$T/tf2/chorus_policy.prom" ] || return 1
}

stale_test() { # $1 = series values for the textfile mtime ("" = absent), $2 = expected alerts
  command -v promtool >/dev/null || { echo "UNMEASURED: no promtool"; return 1; }
  ruby -ryaml -e '
    d = YAML.load_file(ARGV[0])
    r = d["groups"].flat_map { |g| g["rules"] }.find { |x| x["uid"] == "policy-sensor-stale" } or abort "no stale rule"
    q = r["data"].find { |x| x["datasourceUid"] == "prometheus" }["model"]["expr"]
    th = r["data"].find { |x| x["model"]["type"] == "threshold" }["model"]["conditions"][0]["evaluator"]["params"][0]
    sel = q[/node_textfile_mtime_seconds\{[^}]*\}/]
    File.write(ARGV[1], { "groups" => [{ "name" => "p", "rules" => [{ "alert" => r["uid"], "expr" => "(#{q}) > #{th} or absent(#{sel})" }] }] }.to_yaml)' "$RULES" "$T/rules.yml"
  series=""
  [ -n "$1" ] && series="      - { series: 'node_textfile_mtime_seconds{file=\"/x/chorus_policy.prom\"}', values: '$1' }"
  cat > "$T/t.yml" <<Y
rule_files: [rules.yml]
evaluation_interval: 1m
tests:
  - interval: 1m
    input_series:
$series
      - { series: 'up', values: '1' }
    alert_rule_test:
      - eval_time: 0m
        alertname: policy-sensor-stale
        exp_alerts: [$2]
Y
  (cd "$T" && promtool test rules t.yml)
}

@test "the stale rule is quiet while the sensor writes" {
  # promtool evaluates at time 0: an mtime of 0 is 0 h old
  run stale_test "0" ""
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a file 4 h old alerts, and so does a file that never appeared" {
  run stale_test "-14400" "{}"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  run stale_test "" "{}"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: the sensor rows carry the door's create types (hasStep and coversStep are lists)" {
  # the door answers 400 to a scalar on an array field (Borg Queen, 11:29, from the live openapi)
  run python3 -c "import runpy; rows = runpy.run_path('$ROOT/platform/scripts/land-4960-rows')['sensor_rows'](); bad = [(c, n, k) for c, n, w in rows for k in ('hasStep', 'coversStep') if k in w and not isinstance(w[k], list)]; print(bad); raise SystemExit(1 if bad else 0)"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
