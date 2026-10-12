#!/usr/bin/env bats
# @test-type: unit — reads the model, the launchd units and the page files; no store
# @domain: schedule
#
# #4081 — the schedule domain: ScheduledJob is claimed by schedule (not services)
# and lives in its graph; the job's schedule fields are on the shape; the two
# crontab lines are launchd units; the page is the generic rows page with a
# preset, reading only the door. Files read:
#   roles/silas/ontology/chorus.ttl
#   roles/wren/ontology/domains-wren-silas.ttl
#   platform/launchd/com.gathering.backup-pods.plist
#   platform/launchd/com.chorus.cost-metrics.plist
#   platform/api/public/athena/rows.js
#   platform/api/public/athena/rows.html
#   platform/api/public/athena/schedule.html
#   designing/schemas/model-retirements.jsonl

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
DOMAINS="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
LD="$ROOT/platform/launchd"
PAGE="$ROOT/platform/api/public/athena"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#>'

ask() {
  local q="$1"; shift
  local data=(); for f in "$@"; do data+=(--data "$f"); done
  arq "${data[@]}" --results=csv "$P ASK { $q }" 2>/dev/null | tail -1 | tr -d '\r'
}

@test "schedule is the one domain that claims ScheduledJob, and the job lives in its graph" {
  [ "$(ask 'chorus:schedule chorus:definesVocabulary chorus:ScheduledJob ; chorus:atStep ?s ; chorus:ownedBy ?o .
            chorus:ScheduledJobShape chorus:instancesGraph "urn:chorus:domains:schedule" .
            FILTER NOT EXISTS { ?d chorus:definesVocabulary chorus:ScheduledJob FILTER(?d != chorus:schedule) }' "$MODEL" "$DOMAINS")" = "true" ]
}

@test "the job shape carries its schedule: kind, every calendar entry, interval, log, runs, enabled" {
  for p in scheduleKind scheduleSpec intervalSecs logPath runCount enabled; do
    [ "$(ask "chorus:ScheduledJobShape sh:property [ sh:path chorus:$p ]" "$MODEL")" = "true" ] || { echo "missing $p"; false; }
  done
  # scheduleSpec is repeatable: no maxCount, so a second calendar entry is never dropped
  [ "$(ask 'chorus:ScheduledJobShape sh:property ?ps . ?ps sh:path chorus:scheduleSpec ; sh:maxCount ?m' "$MODEL")" = "false" ]
}

@test "the services copies of the 54 jobs retire through the ledger, scoped to the services graph" {
  n=$(grep -c '"graph": "urn:chorus:domains:services".*"card": 4081' "$ROOT/designing/schemas/model-retirements.jsonl")
  [ "$n" -eq 54 ]
}

unit_is() { # unit_is <plist> <program-substring> <schedule key> <value>
  plutil -lint "$1" >/dev/null || return 1
  plutil -convert json -o - "$1" | python3 -I -c '
import json, sys
d = json.load(sys.stdin); prog, key, want = sys.argv[1:4]
args = " ".join(d.get("ProgramArguments", []))
got = json.dumps(d.get(key), sort_keys=True)
ok = prog in args and got == want
print(f"program {args!r}\n{key} {got} (want {want})")
sys.exit(0 if ok else 1)' "$2" "$3" "$4"
}

@test "backup-pods is a launchd unit at 00:00, as its crontab line was" {
  run unit_is "$LD/com.gathering.backup-pods.plist" "scripts/backup-pods.sh" StartCalendarInterval '{"Hour": 0, "Minute": 0}'
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "cost-metrics is a launchd unit every 300 s, as its crontab line was" {
  run unit_is "$LD/com.chorus.cost-metrics.plist" "platform/scripts/cost-metrics" StartInterval '300'
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: a unit on the wrong schedule fails the check" {
  bad="$BATS_TEST_TMPDIR/cost.plist"
  sed 's|<integer>300</integer>|<integer>600</integer>|' "$LD/com.chorus.cost-metrics.plist" > "$bad"
  run unit_is "$bad" "platform/scripts/cost-metrics" StartInterval '300'
  [ "$status" -ne 0 ]
}

generic() { # the rows script names no class and reads only the door
  ! grep -nE 'ScheduledJob|SELECT |/sparql|localhost:3030' "$1"
}

@test "the rows page is generic: no class named in its code, no SPARQL, the door only" {
  run generic "$PAGE/rows.js"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  grep -q 'fetchJSON(' "$PAGE/rows.js"
  grep -q 'src="rows.js"' "$PAGE/rows.html"
  # schedule.html is that page with the kind preset, nothing else of its own
  grep -q 'window.ROWS_KIND="ScheduledJob"' "$PAGE/schedule.html"
  grep -q 'src="rows.js"' "$PAGE/schedule.html"
  if grep -q 'fetch(' "$PAGE/schedule.html"; then echo "schedule.html fetches on its own"; return 1; fi
}

@test "NEGATIVE PROOF: a rows script that names a class fails the generic check" {
  bad="$BATS_TEST_TMPDIR/rows.js"
  { cat "$PAGE/rows.js"; echo "if (kind === 'ScheduledJob') {}"; } > "$bad"
  run generic "$bad"
  [ "$status" -ne 0 ]
}

@test "the schedule page is a tile off /chorus (platform/api/public/index.html)" {
  grep -q 'href="/athena/schedule.html"' "$ROOT/platform/api/public/index.html"
}
