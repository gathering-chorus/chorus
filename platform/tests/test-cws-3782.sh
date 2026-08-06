#!/usr/bin/env bash
# @test-type: unit — fixture pin dirs via CWS_RUNS_DIR, no live state touched; brings its own world (#3528).
# #3782 — /cws status tool: trust rules + negative proof.
set -u
CWS="$(cd "$(dirname "$0")/../scripts" && pwd)/chorus-werk-status"
fails=0
t() { local name="$1" want="$2" got="$3"; if [[ "$got" == *"$want"* ]]; then echo "ok   $name"; else echo "FAIL $name — wanted '$want' in: $got"; fails=$((fails+1)); fi; }

# 1. NEGATIVE PROOF self-test must pass (and is itself the two-states fixture)
out=$("$CWS" --fixture 2>&1); rc=$?
t "fixture negative proof" "NEGATIVE PROOF OK" "$out"
[ $rc -eq 0 ] || { echo "FAIL fixture exit=$rc"; fails=$((fails+1)); }

# 2. hermetic dir: live-running pin (our own pid, fresh startedAt) reports running
TD=$(mktemp -d)
NOW=$(python3 -c "from datetime import datetime,timezone;print(datetime.now(timezone.utc).isoformat())")
cat > "$TD/100.json" <<EOF
{"card":100,"role":"wren","phase":"running","pid":$$,"startedAt":"$NOW","runId":"t-1"}
EOF
t "live pid + fresh start = running" "running" "$(CWS_RUNS_DIR=$TD "$CWS" 100)"

# 3. dead-pid running pin reports abandoned
cat > "$TD/101.json" <<EOF
{"card":101,"role":"wren","phase":"running","pid":999901,"startedAt":"$NOW","runId":"t-2"}
EOF
t "dead pid = abandoned" "abandoned" "$(CWS_RUNS_DIR=$TD "$CWS" 101)"

# 4. old presented pin reports abandoned (expired)
cat > "$TD/102.json" <<EOF
{"card":102,"role":"wren","phase":"presented","presentedAt":"2026-06-16T15:47:19Z","patchId":"x","runId":"t-3"}
EOF
t "expired presented = abandoned" "abandoned" "$(CWS_RUNS_DIR=$TD "$CWS" 102)"

# 5. no-arg / role view hides non-live pins (history is not status)
cat > "$TD/103.json" <<EOF
{"card":103,"role":"kade","phase":"landed","runId":"t-4"}
EOF
t "landed pins hidden from live view" "no live runs" "$(CWS_RUNS_DIR=$TD "$CWS" --role kade)"

rm -rf "$TD"
if [ $fails -gt 0 ]; then echo "test-cws-3782: $fails FAILURE(S)"; exit 1; fi
echo "test-cws-3782: all green"
