#!/usr/bin/env bats
# @test-type: integration — reads Loki (convergence.call events) for the live case; fixtures for the rest
# @domain: alerts
# @card: #4977 · owner: silas
# Checks platform/services/convergence-land/src/lib.rs (call_event repeat=) through its Loki events.
#
# #4977 AC6 — after the land, Grafana holds each of the five row-made rules with its one-year
# repeat wait. #4977 went Proven at 06:07 on 2026-11-15 with none of its rules in Grafana,
# because no check read Grafana (Wren 07:01). convergence-land now reads every landed rule
# back from Grafana's API and refuses a wrong wait; its call event carries repeat=<what
# Grafana holds>. This check reads those events: the newest landing of each rule must be ok
# with repeat=8760h. A rule never landed, refused, or holding another wait fails.

RULES="bedroom-disk-full fuseki-memory-high buzz-shadow-not-matching sensor-run-red shacl-above-baseline"

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

# Loki query_range JSON on stdin; prints one line per rule, exits 1 if any is not held with 8760h
verdict() {
  python3 -c '
import json, sys
want = sys.argv[1].split()
newest = {}
for s in json.load(sys.stdin).get("data", {}).get("result", []):
    for ts, line in s.get("values", []):
        try: e = json.loads(line)
        except ValueError: continue
        if e.get("event") != "convergence.call" or e.get("op") != "alert": continue
        t = e.get("target")
        if t in want and int(ts) > newest.get(t, (0,))[0]:
            newest[t] = (int(ts), e)
bad = 0
for r in want:
    if r not in newest:
        print(f"NEVER LANDED {r}"); bad = 1; continue
    e = newest[r][1]
    if e.get("outcome") != "ok":
        print("REFUSED %s: %s" % (r, e.get("reason", ""))); bad = 1
    elif e.get("repeat") != "8760h":
        print("WRONG WAIT %s: Grafana holds %s" % (r, e.get("repeat", "no read-back"))); bad = 1
    else:
        print(f"held {r} 8760h")
sys.exit(bad)
' "$RULES"
}

event() { # <target> <outcome> [repeat]
  local e="{\"event\":\"convergence.call\",\"op\":\"alert\",\"target\":\"$1\",\"outcome\":\"$2\"${3:+,\"repeat\":\"$3\"}}"
  printf '%s' "$e" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}

fixture() { # lines of: <ts> <target> <outcome> [repeat]
  local vals="" sep=""
  while read -r ts t o r; do
    [ -n "$ts" ] || continue
    vals="$vals$sep[\"$ts\",$(event "$t" "$o" "$r")]"; sep=","
  done
  printf '{"data":{"result":[{"values":[%s]}]}}' "$vals"
}

all_held() { local n=1; for r in $RULES; do echo "$n $r ok 8760h"; n=$((n + 1)); done; }

@test "five rules each landed with Grafana holding 8760h read as held" {
  run bash -c "$(declare -f verdict event fixture all_held); RULES='$RULES'; all_held | fixture | verdict"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(grep -c '^held ' <<<"$output")" -eq 5 ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: each way a rule can be missing from Grafana fails, naming the rule
@test "a rule never landed, refused, read back with no wait, or with 4h fails" {
  for bad in "never" "9 shacl-above-baseline refused" "9 shacl-above-baseline ok" "9 shacl-above-baseline ok 4h"; do
    run bash -c "$(declare -f verdict event fixture all_held); RULES='$RULES'; { all_held | grep -v shacl; [ '$bad' = never ] || echo '$bad'; } | fixture | verdict"
    [ "$status" -eq 1 ] || { echo "$bad: $output"; return 1; }
    [[ "$output" == *"shacl-above-baseline"* ]] || { echo "$output"; return 1; }
  done
}

# NEGATIVE PROOF: the newest landing decides; an old good one does not hide a newer refusal
@test "a newer refused landing after a held one fails" {
  run bash -c "$(declare -f verdict event fixture all_held); RULES='$RULES'; { all_held; echo '99 bedroom-disk-full refused'; } | fixture | verdict"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"REFUSED bedroom-disk-full"* ]] || { echo "$output"; return 1; }
}

# After the land (borg-prove, the nightly): the live events. A card's werk has not landed yet,
# so there the five cannot be held; it says so instead of reading prod.
@test "after the land: Grafana holds the five with 8760h (read back by the land, from Loki)" {
  case "$ROOT" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4977 AC6)";; esac
  local q='{job="platform-chorus"} |= "convergence.call" |= "\"op\":\"alert\""'
  json="$(curl -sf -m 20 -G "${LOKI_URL:-http://localhost:3102}/loki/api/v1/query_range" \
    --data-urlencode "query=$q" --data-urlencode "since=720h" --data-urlencode "limit=5000")" \
    || { echo "Loki unreadable — unmeasured, not held"; return 1; }
  run verdict <<<"$json"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
