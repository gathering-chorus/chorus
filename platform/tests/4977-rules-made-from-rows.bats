#!/usr/bin/env bats
# @test-type: unit — reads the land plan and the rule files; no store, no Grafana
# @domain: monitors
# @card: #4977 · owner: silas
# Checks platform/scripts/land-4977-rows, platform/config/alerting/chorus-sensor-rules.yaml,
# platform/config/alerting/chorus-shacl-rules.yaml,
# platform/scripts/grafana-file-rules-4977 and proving/domains/alerts/.
#
# #4977 AC3 — each of the five rules is made from its rows by the #4970 generator,
# and the old judge of each is removed in the same land: the crawler's yml is
# deleted; the two file rules are replaced by deleteRules (applied by the one
# post-land op); Buzz's rule is replaced in place (same uid); Fuseki's size check
# left chorus-health in #4970.

setup() { ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"; L="$ROOT/platform/scripts/land-4977-rows"; }
# file $1 deletes uid $2 and no group still defines it (ruby's YAML: the 4717 suite's reader;
# the pipeline's python has no PyYAML)
held() {
  ruby -ryaml -e 'd = YAML.load_file(ARGV[0]); u = ARGV[1]
    del = (d["deleteRules"] || []).any? { |r| r["uid"] == u }
    defd = (d["groups"] || []).flat_map { |g| g["rules"] || [] }.any? { |r| r["uid"] == u }
    exit(del && !defd ? 0 : 1)' "$1" "$2"
}

@test "the land makes all five rules from their rows through monitor-alert" {
  for m in crawler-out-of-sync crawler-sync-quiet sensor-run-not-green shacl-above-baseline buzz-shadow fuseki-memory; do
    grep -q "(\"$m-monitor-4977\"," "$L" || { echo "$m not in the plan"; return 1; }
  done
  grep -q 'monitor_alert.build(monitor, sensor)' "$L" || return 1
}
@test "the crawler's old judge is gone: its yml is deleted and its row points at the monitor row" {
  [ ! -e "$ROOT/proving/domains/alerts/crawler-out-of-sync.yml" ] || return 1
  grep -q '"crawler-out-of-sync": "chorus/designing/domains/monitors/crawler-out-of-sync-monitor-4977.json"' "$L" || return 1
}
@test "the two file rules are deleted by uid, and no file still defines them" {
  for pair in "chorus-sensor-rules.yaml sensor-run-not-green" "chorus-shacl-rules.yaml shacl-violations-above-baseline"; do
    set -- $pair
    F="$ROOT/platform/config/alerting/$1"
    held "$F" "$2" || { echo "$1 does not delete $2, or still defines it"; return 1; }
  done
}
@test "NEGATIVE PROOF: the check above catches a file that still defines its rule" {
  T="$BATS_TEST_TMPDIR/bad.yaml"
  printf 'apiVersion: 1\ngroups:\n  - name: g\n    rules:\n      - uid: sensor-run-not-green\ndeleteRules:\n  - orgId: 1\n    uid: sensor-run-not-green\n' > "$T"
  ! held "$T" sensor-run-not-green || return 1
}
@test "the two successors take new uids, so the file deleteRules (which stay for good) never remove them" {
  for pair in "sensor-run-not-green-monitor-4977 sensor-run-red sensor-run-not-green" "shacl-above-baseline-monitor-4977 shacl-above-baseline shacl-violations-above-baseline"; do
    set -- $pair
    [ "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['id'])" "$ROOT/designing/domains/monitors/$1.json")" = "chorus:$2" ] || return 1
    [ "$2" != "$3" ] || return 1
    grep -q "(\"$1\", \"[a-z-]*-4977\", \"on\", True)" "$L" || return 1
  done
}
@test "the post-land op removes the two old file rules and their quiet rows, nothing else" {
  CHORUS_ROOT="$ROOT" OBS_DIR="$BATS_TEST_TMPDIR" run "$ROOT/platform/scripts/grafana-file-rules-4977"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"id=sensor-run-not-green"* ]] || return 1
  [[ "$output" == *"id=shacl-violations-above-baseline"* ]] || return 1
}
@test "the crawler's rows are Kade's: the land skips them, and the same script writes them from his account" {
  python3 - "$L" <<'PY' || return 1
import importlib.machinery, importlib.util, sys
l = importlib.machinery.SourceFileLoader("l4977", sys.argv[1]); s = importlib.util.spec_from_loader("l4977", l)
m = importlib.util.module_from_spec(s); sys.argv = [sys.argv[1]]; l.exec_module(m)
land = [p[0] for p in m.PAIRS]; kade = [p[0] for p in m.KADE_PAIRS]
assert not any(x.startswith("crawler") for x in land), land
assert kade == ["crawler-out-of-sync-monitor-4977", "crawler-sync-quiet-monitor-4977"], kade
src = open(sys.argv[0]).read()
assert '("alerts", "crawler-out-of-sync", None)' in src, "the land does not hand the row over"
PY
}
