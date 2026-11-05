#!/usr/bin/env bats
# @test-type: e2e — after land: reads the live demo witness, no fixture
# @domain: proving
# @card: #4889 · owner: silas
# @needs: nightly — after land: reads the next card's review; the card's AC says `check: after test` (borg-prove re-runs it at the next land)
# Kade 19:53: after #4889 lands, the next card's architecture review must show that
# dependency-cruiser ran (its werk installs it from the lock), not unmeasured.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
W="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/ops/logs/werk-demo.jsonl"

# when #4889 reached main (epoch seconds), or nothing before it lands
land_time() { git -C "$ROOT" log origin/main --first-parent -1 -E --grep='^#4889([^0-9]|$)' --format=%ct 2>/dev/null; }

# the dependency-cruiser part of the first arch review recorded for another card after
# #4889's land ($2, epoch seconds); empty when there is none yet
next_arch_depcruise() {
  python3 - "$1" "$2" <<'PY'
import json, sys
landed_ms = int(sys.argv[2]) * 1000
best = None
for line in open(sys.argv[1], errors="replace"):
    try: e = json.loads(line)
    except ValueError: continue
    if e.get("event") != "demo.gate.result" or e.get("gate") != "arch" or e.get("card_id") == 4889: continue
    ts = e.get("ts", 0)
    if ts > landed_ms and (best is None or ts < best[0]): best = (ts, e.get("findings", ""))
if best is None: print("")
else:
    part = [p for p in best[1].split(" | ") if "dependency-cruiser:" in p]
    # a review with no dependency-cruiser part: it never ran (red)
    print(part[0] if part else "[error] dependency-cruiser could not run: no part in the review")
PY
}

# ok when the part is a dependency-cruiser verdict that was measured (pass or fail)
ran() { [ -n "$1" ] && [[ "$1" != *" error] dependency-cruiser"* && "$1" != *"could not run"* && "$1" != *"no dependency-cruiser report"* ]]; }

@test "AC11 (after land): the next card's architecture review ran dependency-cruiser" {
  t="$(land_time)"
  [ -n "$t" ] || skip "#4889 has not landed yet"
  got="$(next_arch_depcruise "$W" "$t")"
  # none yet is unmeasured (skip), never a pass: borg-prove keeps the deferral open
  [ -n "$got" ] || skip "no architecture review recorded for a card after #4889 yet"
  ran "$got" || { echo "dependency-cruiser did not run: $got"; return 1; }
}

@test "negative proof: an unmeasured dependency-cruiser part is caught" {
  f="$BATS_TEST_TMPDIR/w.jsonl"
  printf '%s\n' '{"ts":1,"event":"demo.gate.result","card_id":4889,"gate":"arch","findings":"x"}' \
    '{"ts":2000,"event":"demo.gate.result","card_id":4900,"gate":"arch","findings":"[part 1/2 pass] gate-arch: no findings | [part 2/2 error] dependency-cruiser: no dependency-cruiser report (exit Some(1)): npm ERR!"}' > "$f"
  got="$(next_arch_depcruise "$f" 1)"
  [[ "$got" == *"no dependency-cruiser report"* ]] || { echo "$got"; return 1; }
  ! ran "$got" || return 1
  ran "[part 2/2 pass] dependency-cruiser: no findings" || return 1   # control
  # a review that has no dependency-cruiser part at all is red, not "not yet"
  printf '%s\n' '{"ts":3000,"event":"demo.gate.result","card_id":4901,"gate":"arch","findings":"gate-arch: no findings"}' > "$f.2"
  got="$(next_arch_depcruise "$f.2" 1)"
  [[ "$got" == *"no part in the review"* ]] || { echo "$got"; return 1; }
  ! ran "$got" || return 1
  # a review recorded before the land never counts
  [ -z "$(next_arch_depcruise "$f.2" 10)" ] || return 1
}
