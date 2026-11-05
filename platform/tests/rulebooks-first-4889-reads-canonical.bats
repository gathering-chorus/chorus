#!/usr/bin/env bats
# @test-type: e2e — after #4894 lands: reads #4889's demo record, no fixture
# @domain: pipelines
# @card: #4889 · owner: silas
# @needs: nightly — after land: reads a quality review recorded after #4894 reached main; the card's AC says `check: after test`
# What Jeff sees: once the rulebooks (#4894) are on main, #4889's quality review reads Semgrep
# from canonical: its record has a semgrep part that is not "not on canonical". Skips
# (unmeasured, never a pass) until #4894 is on main and a review is recorded after it.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
W="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/ops/logs/werk-demo.jsonl"

land_time() { git -C "$ROOT" log origin/main --first-parent -1 -E --grep='^#4894([^0-9]|$)' --format=%ct 2>/dev/null; }

# the semgrep part of #4889's first quality review recorded after $2 (epoch s) in $1; empty if none
semgrep_part() {
  python3 - "$1" "$2" <<'PY'
import json, sys
after = int(sys.argv[2]) * 1000
best = None
for line in open(sys.argv[1], errors="replace"):
    try: e = json.loads(line)
    except ValueError: continue
    if e.get("event") != "demo.gate.result" or e.get("gate") != "quality" or e.get("card_id") != 4889: continue
    ts = e.get("ts", 0)
    if ts > after and (best is None or ts < best[0]): best = (ts, e.get("findings", ""))
if best is None: print("")
else:
    part = [p for p in best[1].split(" | ") if "semgrep:" in p]
    print(part[0] if part else "no semgrep part in the review")
PY
}

reads_canonical() { [ -n "$1" ] && [[ "$1" != *"not on canonical"* && "$1" != *"no semgrep part"* ]]; }

@test "AC (after #4894 lands): #4889's quality review reads Semgrep from canonical" {
  t="$(land_time)"
  [ -n "$t" ] || skip "#4894 has not landed yet"
  got="$(semgrep_part "$W" "$t")"
  [ -n "$got" ] || skip "no #4889 quality review recorded after #4894 landed yet"
  reads_canonical "$got" || { echo "$got"; return 1; }
}

@test "negative proof: a review still on the bootstrap, or with no semgrep part, is caught" {
  f="$BATS_TEST_TMPDIR/w.jsonl"
  printf '%s\n' '{"ts":2000,"event":"demo.gate.result","card_id":4889,"gate":"quality","findings":"[part 1/2 pass] gate-quality: no findings | [part 2/2 error] semgrep: /x/platform/quality/semgrep is not on canonical: this review is unmeasured until it lands"}' > "$f"
  ! reads_canonical "$(semgrep_part "$f" 1)" || return 1
  printf '%s\n' '{"ts":2000,"event":"demo.gate.result","card_id":4889,"gate":"quality","findings":"gate-quality: no findings"}' > "$f"
  ! reads_canonical "$(semgrep_part "$f" 1)" || return 1
  printf '%s\n' '{"ts":2000,"event":"demo.gate.result","card_id":4889,"gate":"quality","findings":"[part 2/2 pass] semgrep: no findings"}' > "$f"
  reads_canonical "$(semgrep_part "$f" 1)" || return 1
  [ -z "$(semgrep_part "$f" 10)" ] || return 1
}
