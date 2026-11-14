#!/usr/bin/env bats
# @test-type: unit — runs chorus-health's own fuseki-memory block against a made-up services listing; no store, no network
# @domain: monitors
# @card: #4970 · owner: silas
# Checks platform/scripts/chorus-health.
#
# #4970 AC5 — Fuseki memory is OFF (Jeff via Wren 2026-11-14 17:47): Fuseki runs
# with -Xmx3g and caps itself, so no size is a fault. The block is read from the
# shipped script (never copied) and run with stub pass/warn/fail: a healthy 2.4 GB
# and an over-heap 3.5 GB both pass with the size reported; a Fuseki missing from
# the registry is still a fail (unmeasured is never fine).
#
# Buzz shadow is recorded, never told, while PULSE_BUZZ_DELIVER is off (Wren's row,
# her go 17:47): the land sets notify off on it, the ICD carries that onto the rule
# as labels.notify, and the webhook records alert.quiet and nudges no one. The
# webhook's both-ways proof is the jest suite this file runs.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TMPD="$(mktemp -d)"
  awk '/^FUSEKI_LABEL=/,/^fi$/' "$ROOT/platform/scripts/chorus-health" > "$TMPD/block.sh"
}
teardown() { rm -rf "$TMPD"; }

# run the shipped block against a services listing; prints "<verdict> <detail>"
judge() {
  mkdir -p "$TMPD/api/api/chorus"
  printf '%s' "$1" > "$TMPD/api/api/chorus/services"
  API_BASE="file://$TMPD/api" bash -c '
    pass() { echo "pass $2"; }; warn() { echo "warn $2"; }; fail() { echo "fail $2"; }
    . "$1"' _ "$TMPD/block.sh"
}

@test "the fuseki-memory block is in the shipped script" {
  [ -s "$TMPD/block.sh" ] || return 1
  grep -q 'fuseki-memory' "$TMPD/block.sh" || return 1
}
@test "a healthy Fuseki at 2.4 GB passes, size reported" {
  run judge '{"services":[{"label":"com.gathering.fuseki","rss_mb":2444}]}'
  [ "$output" = "pass rss=2444MB" ]
}
@test "Fuseki over its old 2000 MB ceiling and over its heap is not a fault: no warn, no alert" {
  run judge '{"services":[{"label":"com.gathering.fuseki","rss_mb":3500}]}'
  [ "$output" = "pass rss=3500MB" ]
}
@test "NEGATIVE PROOF: a Fuseki missing from the registry still fails (unmeasured, never fine)" {
  run judge '{"services":[{"label":"com.gathering.fuseki-backup","rss_mb":0}]}'
  [[ "$output" == fail\ UNMEASURED* ]] || return 1
}
@test "NEGATIVE PROOF: main's block warned at 2444 MB, so this suite can tell the two apart" {
  git -C "$ROOT" show origin/main:platform/scripts/chorus-health > "$TMPD/main.sh" 2>/dev/null || skip "origin/main not readable"
  { echo 'FUSEKI_MEM_CEILING_MB=2000'; awk '/^FUSEKI_LABEL=/,/^fi$/' "$TMPD/main.sh"; } > "$TMPD/block.sh"
  run judge '{"services":[{"label":"com.gathering.fuseki","rss_mb":2444}]}'
  [[ "$output" == warn* ]] || return 1
}

@test "Buzz shadow: the land sets notify off on Wren's row and re-lands its rule" {
  L="$ROOT/platform/scripts/land-4970-rows"
  grep -q 'BUZZ = "buzz-shadow-not-matching"' "$L" || return 1
  grep -q '("alerts", BUZZ, {"notify": "off"})' "$L" || return 1
  grep -q 'land_rule(BUZZ)' "$L" || return 1
}
@test "Buzz shadow: the ICD carries notify onto the rule's labels, and the shape takes only on or off" {
  I="$ROOT/building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl"
  awk '/<icd\/mapping\/alerts\/notify> a icd:FieldMapping/,/fromProvider/' "$I" | grep -q 'icd:sourceField "labels.notify"' || return 1
  grep -q 'sh:path chorus:notify ; sh:maxCount 1 ; sh:in ( "on" "off" )' "$ROOT/roles/silas/ontology/alerts-4085.ttl" || return 1
}
@test "Buzz shadow: the webhook records notify off and nudges no one; absent or on still tells (jest, both ways)" {
  cd "$ROOT/platform/api"
  run npx jest tests/handlers/grafana-alerts.test.ts -t 'notify'
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"2 passed"* ]] || return 1
}
