#!/usr/bin/env bats
# @test-type: unit — reads the land plan and the rule files; no store, no Grafana
# @domain: monitors
# @card: #4977 · owner: silas
# Checks platform/scripts/land-4977-rows, platform/config/alerting/*.yaml,
# platform/scripts/grafana-file-rules-4977 and proving/domains/alerts/.
#
# #4977 AC3 — each of the five rules is made from its rows by the #4970 generator,
# and the old judge of each is removed in the same land: the crawler's yml is
# deleted; the two file rules are replaced by deleteRules (applied by the one
# post-land op); Buzz's rule is replaced in place (same uid); Fuseki's size check
# left chorus-health in #4970.

setup() { ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"; L="$ROOT/platform/scripts/land-4977-rows"; }

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
    python3 -c "
import yaml,sys
d=yaml.safe_load(open(sys.argv[1])); uid=sys.argv[2]
assert any(r.get('uid')==uid for r in d.get('deleteRules',[])), 'no deleteRules for '+uid
assert not any(r.get('uid')==uid for g in d.get('groups',[]) for r in g.get('rules',[])), uid+' still defined'
" "$F" "$2" || return 1
  done
}
@test "NEGATIVE PROOF: the check above catches a file that still defines its rule" {
  T="$BATS_TEST_TMPDIR/bad.yaml"
  printf 'apiVersion: 1\ngroups:\n  - name: g\n    rules:\n      - uid: sensor-run-not-green\ndeleteRules:\n  - orgId: 1\n    uid: sensor-run-not-green\n' > "$T"
  ! python3 -c "
import yaml,sys
d=yaml.safe_load(open(sys.argv[1]))
assert not any(r.get('uid')=='sensor-run-not-green' for g in d.get('groups',[]) for r in g.get('rules',[]))
" "$T" 2>/dev/null || return 1
}
@test "the held two land their rules only after the post-land op, and stay quiet until then" {
  grep -q '("sensor-run-not-green-monitor-4977", "sensor-runs-sensor-4977", "off", False)' "$L" || return 1
  grep -q '("shacl-above-baseline-monitor-4977", "shacl-above-baseline-sensor-4977", "off", False)' "$L" || return 1
  CHORUS_ROOT="$ROOT" OBS_DIR="$BATS_TEST_TMPDIR" run "$ROOT/platform/scripts/grafana-file-rules-4977"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"convergence-land-alert alert=sensor-run-not-green"* ]] || return 1
  [[ "$output" == *"notify=on"* ]] || return 1
}
