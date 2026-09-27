#!/usr/bin/env bash
# @test-type: unit — fixture pin dirs via CWS_RUNS_DIR, no live state touched; brings its own world (#3528).
# @domain: cicd — the product domain this suite guards (#4334)
# #3782 — /cws status tool: trust rules + negative proof.
set -u
CWS="$(cd "$(dirname "$0")/../scripts" && pwd)/chorus-werk-status"
fails=0
# #4341 — the board, the spine and the card CLI all point nowhere: this suite reads only its fixtures
export CWS_API=http://127.0.0.1:9 CWS_LOKI=http://127.0.0.1:9 CWS_CARDS=/usr/bin/false
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
t "no-arg view says when the board cannot be read" "board unreadable" "$(CWS_RUNS_DIR=$TD "$CWS" --role kade)"

# 6. #4341 NEGATIVE PROOF — a run that FINISHED red is failed, never abandoned.
# The #4334 pin said running with a dead pid after its test step went red at
# 13:11; the tool called it abandoned. The log's WERK_EXIT is the fact.
cat > "$TD/104.json" <<EOF
{"card":104,"role":"kade","phase":"running","pid":999902,"startedAt":"$NOW","runId":"104-t-5"}
EOF
cat > "$TD/104-104-t-5.log" <<'EOF'
[werk/werk]   ✅  Success - Main build [33.5s]
[werk/werk]   |    bats:platform/tests/4166-athena-validate-scheduled.bats … FAIL
[werk/werk]   |    bats:platform/tests/standards-gen.bats … FAIL
[werk/werk]   ❌  Failure - Main test [27m28.4s]
[werk/werk]   ✅  Success - Main witness-failed [15ms]
WERK_EXIT=1
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 104)
t "finished red run = failed" "#104 [kade] failed" "$out"
if [[ "$out" == *abandoned* ]]; then echo "FAIL finished red run still reads abandoned"; fails=$((fails+1)); fi
t "run row names the failed step and time" "failed  test (27m)" "$out"
t "run row names the reds" "2 reds: 4166-athena-validate-scheduled, standards-gen" "$out"
t "quiet steps are not rows" "1 run" "$out"
t "unknown times say so" "cycle unknown" "$out"

rm -rf "$TD"
if [ $fails -gt 0 ]; then echo "test-cws-3782: $fails FAILURE(S)"; exit 1; fi
echo "test-cws-3782: all green"
