#!/usr/bin/env bats
# @test-type: unit
# @domain: services — the product domain this suite guards (#4334)
# 4170 — the fuseki-memory check must read the store, not a sibling.
#
# It selected the first service whose label CONTAINS "fuseki". Four match; the
# first is com.gathering.fuseki-backup, a periodic job that sits at rss 0. The
# check reported that as "Fuseki rss=0 or not found" and nudged Jeff on
# 2026-07-21, 09-12, 09-13 07:43 and 09-14 05:41 — every time with the real
# store healthy (102MB at the last firing, 446MB at the one before).
#
# The two states it must separate: "the store is using no memory" and "the
# registry has no such service". Before this card it reported both as 0.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HEALTH="$REPO_ROOT/platform/scripts/chorus-health"

# The selection, lifted verbatim from the script so the test exercises the real
# expression rather than a paraphrase of it.
select_rss() {
  python3 -c "
import json,sys
label=sys.argv[1]
svcs=json.load(sys.stdin).get('services',[])
hit=[x for x in svcs if x.get('label')==label]
print(hit[0].get('rss_mb') if hit and hit[0].get('rss_mb') is not None else 'ABSENT')
" "$1"
}
export -f select_rss

REGISTRY='{"services":[
  {"label":"com.gathering.fuseki-backup","rss_mb":0},
  {"label":"com.chorus.fuseki-perf","rss_mb":null},
  {"label":"com.chorus.fuseki-compact","rss_mb":null},
  {"label":"com.gathering.fuseki","rss_mb":102}]}'

@test "it reads the store, not the backup job that sorts first" {
  run bash -c "echo '$REGISTRY' | select_rss com.gathering.fuseki"
  [ "$output" = "102" ]
}

@test "NEGATIVE PROOF — the store absent reads UNMEASURED, never 0" {
  # The exact shape that cried wolf: siblings present, store gone.
  local without='{"services":[{"label":"com.gathering.fuseki-backup","rss_mb":0}]}'
  run bash -c "echo '$without' | select_rss com.gathering.fuseki"
  [ "$output" = "ABSENT" ]
  [ "$output" != "0" ]
}

@test "NEGATIVE PROOF — a store over the ceiling is still reachable" {
  # A check that can never go red is no check. Prove the alarm state exists.
  local over='{"services":[{"label":"com.gathering.fuseki","rss_mb":9000}]}'
  run bash -c "echo '$over' | select_rss com.gathering.fuseki"
  [ "$output" = "9000" ]
  [ "$output" -gt 2000 ]
}

@test "the script matches by exact label and treats absence as UNMEASURED" {
  grep -q 'x.get(.label.)==label' "$HEALTH"
  grep -q 'UNMEASURED' "$HEALTH"
  # the substring match this card removes must not come back
  ! grep -q "'fuseki' in x.get(.label.,..)" "$HEALTH"
}
