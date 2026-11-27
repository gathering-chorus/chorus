#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-5042-rows against a fixture door (LAND_LIVE_JSON); no store, no runner
# @domain: services
# @card: #5042 AC2 · owner: kade
#
# Jeff, on the principles domain page, 2026-11-25 10:28: "i expect to see principles service here
# yes?". athena-gen writes each generated API's service facts to its crate's surfaces/service.json;
# platform/scripts/land-5042-rows writes them through the door. This holds, offline, on the files
# the generator wrote in this tree:
#   - nothing there yet         → each Service row is added and each hosting Domain row changed;
#   - everything there already  → nothing to write (a second land plans nothing);
#   - NEGATIVE: the hosts edge left out is a change, and a dry run calls no runner;
#   - no foldNote is ever written: a repeated text field, which the door refuses to patch;
#   - the rows are planned for the hosting Domain's owner only, and every other owner is named;
#   - the unit map ties each generated unit to the service the generator names.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).
#
# Covers: platform/scripts/land-5042-rows
# Covers: platform/config/service-instance-map.json

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/scripts/land-5042-rows"
MAP="$ROOT/platform/config/service-instance-map.json"
PRINCIPLES="$ROOT/shaping/domains/principles/api/surfaces/service.json"
PULSE="$ROOT/directing/products/pulse/api/surfaces/service.json"
STALE="Services: principles-api serves this domain (com.chorus.principles-api, #5017), but no Service row names it yet, so chorus:hosts has nothing to point at."

setup() {
  T="$(mktemp -d)"
  printf '#!/bin/bash\necho called >> "%s/runner.log"\n' "$T" > "$T/runner"
  chmod +x "$T/runner"
}
teardown() { rm -rf "$T"; }

# the fixture door: $1 = "none" | "done" Service rows, $2 = hosts on principles, $3 = "stale" | "kept" note
door() {
  python3 - "$PRINCIPLES" "$PULSE" "$1" "$2" "$3" "$STALE" > "$T/live.json" <<'PY'
import json, sys
principles, pulse, service, hosts, notes, stale = sys.argv[1:7]
facts = [json.load(open(principles)), json.load(open(pulse))]
kept = ["Decisions: 0 because no decision carries hasDomain yet; #4464 (decisions cleanup) owns that."]
domains = [
    {"name": "principles", "ownedBy": "principal-wren", "hosts": hosts, "foldNote": kept + ([stale] if notes == "stale" else [])},
    {"name": "awareness", "ownedBy": "principal-wren", "hosts": "service-pulse-api" if service == "done" else ""},
]
services = [dict(f["service"]) for f in facts] if service == "done" else []
print(json.dumps({"services": services, "domains": domains}))
PY
  LAND_LIVE_JSON="$T/live.json" LAND_SERVICE_FILES="$PRINCIPLES:$PULSE" CHORUS_RUNNER="$T/runner" run python3 "$LAND" --as "${AS:-wren}"
}

@test "#5042 the generator names principles-api hosted by principles and pulse-api hosted by awareness" {
  python3 -c 'import json,sys; f=json.load(open(sys.argv[1])); assert (f["service"]["name"], f["hostedBy"]["name"], f["hostedBy"]["hosts"], f["unit"]) == ("principles-api", "principles", "service-principles-api", "com.chorus.principles-api"), f' "$PRINCIPLES"
  python3 -c 'import json,sys; f=json.load(open(sys.argv[1])); assert (f["service"]["name"], f["hostedBy"]["name"], f["unit"]) == ("pulse-api", "awareness", "com.chorus.pulse-api"), f' "$PULSE"
}

@test "#5042 nothing there yet: each Service row is added and each hosting Domain row changed" {
  door none "" stale
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^ADD    services/principles-api' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^ADD    services/pulse-api' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^CHANGE domains/principles' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^CHANGE domains/awareness .*"hosts": \["service-pulse-api"\]' || { echo "$output"; return 1; }
  echo "$output" | grep -q '2 add, 2 change, 0 delete; 0 same' || { echo "$output"; return 1; }
}

@test "#5042 everything there already, in the form the door serves it: a second land plans nothing" {
  door done "service-principles-api" kept
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '0 add, 0 change, 0 delete; 2 same' || { echo "$output"; return 1; }
}

@test "#5042 NEGATIVE: no hosts edge is a change, and a dry run calls no runner" {
  door done "" kept
  echo "$output" | grep -q '0 add, 1 change, 0 delete; 2 same' || { echo "a missing hosts edge read as same: $output"; return 1; }
  echo "$output" | grep -q '^CHANGE domains/principles .*"hosts": \["service-principles-api"\]' || { echo "$output"; return 1; }
  test ! -e "$T/runner.log" || { echo "a dry run called the runner"; return 1; }
}

@test "#5042 no foldNote is planned: the door refuses a patch that sets a repeated text field" {
  door none "" stale
  echo "$output" | grep -q 'foldNote' && { echo "a foldNote write was planned: $output"; return 1; }
  return 0
}

@test "#5042 a host already hosting another service keeps it beside the new one" {
  door done "service-other" kept
  echo "$output" | grep -q '^CHANGE domains/principles .*"hosts": \["service-other", "service-principles-api"\]' || { echo "$output"; return 1; }
}

@test "#5042 the plan is the hosting Domain owner's: another role, even with --apply, writes nothing and is told who runs it" {
  # the land runs the script as the card's role (kade): Wren's rows must plan as OTHER, never be refused (#5045's stop)
  door none "" stale
  LAND_LIVE_JSON="$T/live.json" LAND_SERVICE_FILES="$PRINCIPLES:$PULSE" CHORUS_RUNNER="$T/runner" run python3 "$LAND" --as kade --apply
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^OTHER  wren: 4 row(s), run as wren' || { echo "$output"; return 1; }
  echo "$output" | grep -q '0 add, 0 change, 0 delete; 0 same — applied' || { echo "$output"; return 1; }
  test ! -e "$T/runner.log" || { echo "a run as kade called the runner"; return 1; }
}

@test "#5042 NEGATIVE: a service.json that names no host is refused, never planned without its hosts edge" {
  python3 -c 'import json,sys; f=json.load(open(sys.argv[1])); f["hostedBy"]=None; json.dump(f, open(sys.argv[2],"w"))' "$PRINCIPLES" "$T/nohost.json"
  door none "" kept
  LAND_LIVE_JSON="$T/live.json" LAND_SERVICE_FILES="$T/nohost.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND" --as wren
  test "$status" -eq 2 || { echo "$output"; return 1; }
  echo "$output" | grep -q 'names no Domain row that hosts it' || { echo "$output"; return 1; }
}

@test "#5042 the unit map ties each generated unit to the service the generator names, and no longer to athena or clearing" {
  for f in "$PRINCIPLES" "$PULSE"; do
    python3 -c 'import json,sys; f=json.load(open(sys.argv[1])); m=json.load(open(sys.argv[2])); want="service-"+f["service"]["name"]; got=m.get(f["unit"]); assert got==want, (f["unit"], got, want)' "$f" "$MAP" || return 1
  done
}
