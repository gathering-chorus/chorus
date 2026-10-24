#!/usr/bin/env bats
# @test-type: unit — promtool evaluates the rules against synthetic series; no Prometheus, no Grafana.
# @domain: alerts
# @card: #4717
# @owner: silas
#
# #4717 AC4 — platform/config/alerting/chorus-shacl-rules.yaml (Grafana, deployed
# to shared-observability) fires on exactly the states Jeff asked for: the store's
# violation count above its baseline (the ratchet), an empty shared blank node in
# the shapes, and a census or gauge that stopped writing. Each rule's query and
# threshold is read from the file itself and evaluated by `promtool test rules`,
# so a rule that is edited to never fire turns this red. Negative proofs: at the
# baseline, at 0, and with a fresh file, nothing fires.

ROOT="$BATS_TEST_DIRNAME/../.."
RULES="$ROOT/platform/config/alerting/chorus-shacl-rules.yaml"

setup() {
  command -v promtool >/dev/null || skip "no promtool"
  T="$(mktemp -d)"
  # the Grafana rule as a Prometheus alert: A (minus B for a math rule) against
  # the threshold, and `absent()` of the series when no data must alert too
  ruby -ryaml -e '
    d = YAML.load_file(ARGV[0])
    rules = d["groups"].flat_map { |g| g["rules"] }.map do |r|
      q = r["data"].select { |x| x["datasourceUid"] == "prometheus" }.map { |x| x["model"]["expr"] }
      th = r["data"].find { |x| x["model"]["type"] == "threshold" }["model"]["conditions"][0]["evaluator"]
      math = r["data"].any? { |x| x["model"]["type"] == "math" }
      expr = math ? "(#{q[0]}) - (#{q[1]}) > #{th["params"][0]}" : "(#{q[0]}) > #{th["params"][0]}"
      if r["noDataState"] == "Alerting"
        sel = q[0][/node_textfile_mtime_seconds\{[^}]*\}/] or abort "#{r["uid"]}: no-data alerting but no textfile selector"
        expr = "#{expr} or absent(#{sel})"
      end
      { "alert" => r["uid"], "expr" => expr }
    end
    File.write(ARGV[1], { "groups" => [{ "name" => "shacl", "rules" => rules }] }.to_yaml)' "$RULES" "$T/rules.yml"
}

teardown() { if [ -n "${T:-}" ]; then rm -rf "$T"; fi; }

# test <alert> <eval_time> <expected: firing|none> <series yaml lines...>
check() {
  local alert="$1" at="$2" want="$3"; shift 3
  { echo "rule_files: [rules.yml]"; echo "evaluation_interval: 1m"; echo "tests:"; echo "  - interval: 1m"; echo "    input_series:"
    for s in "$@"; do echo "      - $s"; done
    echo "    alert_rule_test:"; echo "      - eval_time: $at"; echo "        alertname: $alert"
    if [ "$want" = firing ]; then echo "        exp_alerts: [{}]"; else echo "        exp_alerts: []"; fi
  } > "$T/t-$alert.yml"
  (cd "$T" && promtool test rules "t-$alert.yml") || { cat "$T/rules.yml" "$T/t-$alert.yml"; return 1; }
}

@test "the rules file holds the three alerts AC4 names, and every one routes to a role" {
  for uid in shacl-violations-above-baseline shacl-shared-bnodes-empty shacl-census-stale shacl-bnodes-stale; do
    grep -q "uid: $uid" "$RULES" || { echo "missing $uid"; return 1; }
  done
  [ "$(grep -c 'role: silas' "$RULES")" -ge 4 ] || return 1
  # Jeff 10-24 06:03: the summary carries words and values, never a refId
  ! grep -E "summary:.*\b[A-Z]=" "$RULES" || return 1
}

@test "a violation count above the baseline fires; at the baseline it does not" {
  check shacl-violations-above-baseline 5m firing \
    "{series: 'athena_shacl_violations', values: '23047x10'}" "{series: 'athena_shacl_violations_baseline', values: '23000x10'}"
  check shacl-violations-above-baseline 5m none \
    "{series: 'athena_shacl_violations', values: '23000x10'}" "{series: 'athena_shacl_violations_baseline', values: '23000x10'}"
}

@test "an empty shared blank node fires; 0 does not" {
  check shacl-shared-bnodes-empty 5m firing "{series: 'athena_shared_bnodes_empty', values: '1x10'}"
  check shacl-shared-bnodes-empty 5m none "{series: 'athena_shared_bnodes_empty', values: '0x10'}"
}

@test "a census file 27 h old fires, a missing one fires, a fresh one does not" {
  # promtool's clock starts at 0: an mtime of 0 read at 27 h is 27 h old
  check shacl-census-stale 27h firing "{series: 'node_textfile_mtime_seconds{file=\"/x/athena_shacl.prom\"}', values: '0x1700'}"
  check shacl-census-stale 5m firing "{series: 'node_textfile_mtime_seconds{file=\"/x/other.prom\"}', values: '0x10'}"
  check shacl-census-stale 5m none "{series: 'node_textfile_mtime_seconds{file=\"/x/athena_shacl.prom\"}', values: '0x10'}"
}

@test "a blank-node gauge 3 h old fires; one 1 h old does not" {
  check shacl-bnodes-stale 3h firing "{series: 'node_textfile_mtime_seconds{file=\"/x/athena_shacl_bnodes.prom\"}', values: '0x200'}"
  check shacl-bnodes-stale 1h none "{series: 'node_textfile_mtime_seconds{file=\"/x/athena_shacl_bnodes.prom\"}', values: '0x200'}"
}
