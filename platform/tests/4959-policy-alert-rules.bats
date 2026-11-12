#!/usr/bin/env bats
# @test-type: unit — promtool evaluates the rule against synthetic series; no Prometheus, no Grafana.
# @domain: policies
# @card: #4959
# @owner: silas
#
# #4959 AC3 — platform/config/alerting/chorus-policy-rules.yaml fires for a policy
# above its baseline and only that one. The query and threshold are read from the
# file, so a rule edited to never fire turns this red. Negative proof: at the
# baseline nothing fires.

ROOT="$BATS_TEST_DIRNAME/../.."
RULES="$ROOT/platform/config/alerting/chorus-policy-rules.yaml"

setup() {
  command -v promtool >/dev/null || skip "no promtool"
  T="$(mktemp -d)"
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
    File.write(ARGV[1], { "groups" => [{ "name" => "policies", "rules" => rules }] }.to_yaml)' "$RULES" "$T/rules.yml"
}

teardown() { if [ -n "${T:-}" ]; then rm -rf "$T"; fi; }

promtest() {
  cat > "$T/t.yml" <<Y
rule_files: [rules.yml]
evaluation_interval: 1m
tests:
  - interval: 1m
    input_series:
      - { series: 'chorus_policy_violations{policy="a",gate="arch"}', values: '$1' }
      - { series: 'chorus_policy_baseline{policy="a",gate="arch"}', values: '$2' }
      - { series: 'chorus_policy_violations{policy="b",gate="data"}', values: '0' }
      - { series: 'chorus_policy_baseline{policy="b",gate="data"}', values: '0' }
    alert_rule_test:
      - eval_time: 0m
        alertname: policy-violations-above-baseline
        exp_alerts: [$3]
Y
  (cd "$T" && promtool test rules t.yml)
}

@test "one violation over the baseline fires for that policy only" {
  run promtest 4 3 '{ exp_labels: { policy: a, gate: arch } }'
  [ "$status" -eq 0 ]
}

@test "negative: at the baseline nothing fires" {
  run promtest 3 3 ''
  [ "$status" -eq 0 ]
}

@test "negative proof of the test: expecting no alert while one is over the baseline goes red" {
  run promtest 4 3 ''
  [ "$status" -ne 0 ]
}
