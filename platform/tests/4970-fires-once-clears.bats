#!/usr/bin/env bats
# @test-type: unit — evaluates the generated rule query with promtool over made-up series; no Grafana, no network
# @domain: monitors
# @card: #4970 · owner: silas
# Checks platform/scripts/monitor-alert and building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl.
#
# #4970 AC4 — negative proof both ways, on the exact query the generator makes:
# a volume at 94.06% sends nothing; a volume at 99.72% fires, naming the volume;
# while it stays over, Grafana is told not to tell again (repeat_interval 8760h,
# mapped by the ICD onto the rule's own notification_settings); dropping under
# the line clears it. promtool evaluates the same PromQL Grafana runs against
# Prometheus; the fixtures carry node-exporter's real label shape.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export ROOT TMPD="$(mktemp -d)"
  command -v promtool >/dev/null || { echo "promtool not on PATH" >&2; return 1; }
  F="$ROOT/designing/domains/monitors"
  "$ROOT/platform/scripts/monitor-alert" build "$F/bedroom-disk-full-monitor-4970.json" "$F/infra-disk-sensor-4970.json" > "$TMPD/alert.json"
  Q="$(python3 -c "import json;print(json.load(open('$TMPD/alert.json'))['metricQuery'])")"
  python3 - "$Q" "$TMPD" <<'PY'
import json, sys
q, d = sys.argv[1], sys.argv[2]
rules = {"groups": [{"name": "g", "rules": [{"alert": "disk-full", "expr": q}]}]}
json.dump(rules, open(f"{d}/rules.yml", "w"))
L = lambda mp: '{__name__="%s",instance="192.0.2.10:9100",fstype="apfs",mountpoint="%s"}'
size = 1000
def series(mp, used):  # used % per minute -> avail and size series
    return [{"series": (L(mp) % ("node_filesystem_size_bytes", mp)), "values": " ".join([str(size)] * len(used))},
            {"series": (L(mp) % ("node_filesystem_avail_bytes", mp)), "values": " ".join(str(round(size * (100 - u) / 100, 1)) for u in used)}]
s = series("/Volumes/VideosNew", [94.06] * 6) + series("/Volumes/VideosKem-Lat", [94.0, 99.72, 99.72, 99.72, 98.0, 98.0])
lbl = {"instance": "192.0.2.10:9100", "fstype": "apfs", "mountpoint": "/Volumes/VideosKem-Lat"}
t = {"rule_files": ["rules.yml"], "evaluation_interval": "1m", "tests": [{"interval": "1m", "input_series": s, "alert_rule_test": [
    {"eval_time": "0m", "alertname": "disk-full", "exp_alerts": []},
    {"eval_time": "1m", "alertname": "disk-full", "exp_alerts": [{"exp_labels": lbl}]},
    {"eval_time": "3m", "alertname": "disk-full", "exp_alerts": [{"exp_labels": lbl}]},
    {"eval_time": "4m", "alertname": "disk-full", "exp_alerts": []},
]}]}
json.dump(t, open(f"{d}/test.yml", "w"))
PY
  (cd "$TMPD" && promtool test rules test.yml > "$TMPD/out" 2>&1; echo $? > "$TMPD/rc")
}
teardown_file() { rm -rf "$TMPD"; }

@test "promtool ran the generated query: quiet at 94.06, fires for the volume at 99.72 and only it, clears under the line" {
  cat "$TMPD/out"
  [ "$(cat "$TMPD/rc")" = 0 ]
}
@test "NEGATIVE PROOF: the same series against a line of 94 fires for VideosNew too, so the test can go red" {
  sed 's/) > 99"/) > 94"/' "$TMPD/rules.yml" > "$TMPD/rules94.yml"
  sed 's/rules.yml/rules94.yml/' "$TMPD/test.yml" > "$TMPD/test94.yml"
  cd "$TMPD"; run promtool test rules test94.yml
  [ "$status" -ne 0 ]
  [[ "$output" == *VideosNew* ]] || return 1
}
@test "while it stays over, the rule is told not to tell again: repeatInterval 8760h" {
  [ "$(python3 -c "import json;print(json.load(open('$TMPD/alert.json'))['repeatInterval'])")" = "8760h" ]
}
@test "the ICD maps repeatInterval onto the rule's own notification_settings" {
  I="$ROOT/building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl"
  grep -q 'icd:hasMapping <icd/mapping/alerts/repeatInterval>' "$I"
  awk '/<icd\/mapping\/alerts\/repeatInterval> a icd:FieldMapping/,/fromProvider/' "$I" | grep -q 'icd:sourceField "notification_settings.repeat_interval"'
}
