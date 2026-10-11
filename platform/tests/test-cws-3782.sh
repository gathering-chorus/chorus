#!/usr/bin/env bash
# @test-type: unit — fixture pin dirs via CWS_RUNS_DIR, no live state touched; brings its own world (#3528).
# @domain: cicd — the product domain this suite guards (#4334)
# #3782 — /cws status tool: trust rules + negative proof.
set -u
CWS="$(cd "$(dirname "$0")/../scripts" && pwd)/chorus-werk-status"
fails=0
# #4341 — the board, the spine and the card CLI all point nowhere: this suite reads only its fixtures
export CWS_API=http://127.0.0.1:9 CWS_LOKI=http://127.0.0.1:9 CWS_CARDS=/usr/bin/false CWS_DAGU=http://127.0.0.1:9
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

# #4420 — a running run is "at" the step act STARTED last, not the last one
# that finished (Jeff 10-06: "your card is not in build its a cws bug").
# Negative proof: the old rule read the last Success line and said "at build".
cat > "$TD/105.json" <<EOF
{"card":105,"role":"kade","phase":"running","pid":$$,"startedAt":"$NOW","runId":"105-t-6"}
EOF
cat > "$TD/105-105-t-6.log" <<'EOF'
[werk/werk] ⭐ Run Main build
[werk/werk]   ✅  Success - Main build [1m9.8s]
[werk/werk] ⭐ Run Main test
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 105)
t "a running run is at the step in flight" "at test" "$out"
if [[ "$out" == *"at build"* ]]; then echo "FAIL running run still reads at build"; fails=$((fails+1)); fi

# #4420 reopened (Wren, #4438) — a cancelled run reads cancelled, not failed,
# and a red test step names the tests that went red, never a selection line.
# Negative proof: the old "why" matched "refus" in a jest-select line and named
# envelope-refusal-metric-3628, which never failed.
cat > "$TD/106.json" <<EOF
{"card":106,"role":"kade","phase":"running","pid":999903,"startedAt":"$NOW","runId":"106-t-7"}
EOF
cat > "$TD/106-106-t-7.log" <<'EOF'
[werk/werk] ⭐ Run Main test
[werk/werk]   ❌  Failure - Main test [3m1.0s]
Error: context canceled
WERK_EXIT=cancelled
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 106)
t "a cancelled run reads cancelled" "#106 [kade] cancelled" "$out"
t "its row says cancelled at the step" "cancelled               at test" "$out"
if [[ "$out" == *failed* ]]; then echo "FAIL cancelled run reads failed"; fails=$((fails+1)); fi

cat > "$TD/107.json" <<EOF
{"card":107,"role":"kade","phase":"running","pid":999904,"startedAt":"$NOW","runId":"107-t-8"}
EOF
cat > "$TD/107-107-t-8.log" <<'EOF'
[werk/werk]   | jest-select:   platform/api/tests/envelope-refusal-metric-3628.test.ts (domain:security)
[werk/werk]   | !! jest:platform/api WHY: platform/api/tests/search-freshness.integration.test.ts :: x :: Error: thrown
[werk/werk]   |    jest:platform/api … FAIL 1m03s
[werk/werk]   |    lint-ratchet:workspace … FAIL 13.6s
[werk/werk]   ❌  Failure - Main test [58m32.9s]
WERK_EXIT=1
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 107)
t "the why names the tests that went red" "2 reds: search-freshness.integration.test, lint-ratchet:workspace" "$out"
if [[ "$out" == *envelope-refusal* ]]; then echo "FAIL why names a selection line"; fails=$((fails+1)); fi

# The exit marker is its own line. Negative proof: #4420's own run printed AC
# text quoting WERK_EXIT=cancelled and the running run read "stopped".
cat > "$TD/108.json" <<EOF
{"card":108,"role":"kade","phase":"running","pid":$$,"startedAt":"$NOW","runId":"108-t-9"}
EOF
cat > "$TD/108-108-t-9.log" <<'EOF'
[werk/werk]   |   unchecked AC (1): A log ending WERK_EXIT=cancelled reads "cancelled"
[werk/werk] ⭐ Run Main deploy-werk
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 108)
t "a quoted marker is not an exit" "running                 at deploy-werk" "$out"

# #4474 — werk v2 runs come from dagu (a file:// fixture stands in for its API).
# Negative proof: a run for CARD=1090 is a different card and must not count.
# #4494 — both workflows come from dagu's one run list (cicd and cicd-land by name).
mkdir -p "$TD/dagu/api/v1/dags/cicd" "$TD/dagu/api/v1/queues/werk-land"
cat > "$TD/dagu/api/v1/dag-runs" <<'EOF'
{"dagRuns":[
 {"name":"cicd","dagRunId":"r3","params":"CARD=1090 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-09T23:00:00-04:00","finishedAt":"2026-10-09T23:01:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-build"}}]},
 {"name":"cicd","dagRunId":"r2","params":"CARD=109 ROLE=kade","statusLabel":"waiting","startedAt":"2026-10-09T22:14:19-04:00","finishedAt":"2026-10-09T22:54:28-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-demo"}},{"statusLabel":"waiting","step":{"name":"skill-werk-go"}}]},
 {"name":"cicd","dagRunId":"r1","params":"CARD=109 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-09T21:03:31-04:00","finishedAt":"2026-10-09T21:03:32-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-commit"}},{"statusLabel":"aborted","step":{"name":"skill-werk-push"}}]},
 {"name":"cicd","dagRunId":"r0","params":"CARD=109 ROLE=kade","statusLabel":"waiting","startedAt":"2026-10-09T21:00:00-04:00","finishedAt":"2026-10-09T21:02:00-04:00","nodes":[{"statusLabel":"waiting","step":{"name":"skill-werk-go","id":"skill_werk_go"}}]}
]}
EOF
# NEGATIVE PROOF for the count: the per-workflow list stops at dagu's 30 newest
# runs; a decoy there holding one run for 109 must not become the count.
cat > "$TD/dagu/api/v1/dags/cicd/dag-runs" <<'EOF'
{"dagRuns":[{"name":"cicd","dagRunId":"r2","params":"CARD=109 ROLE=kade","statusLabel":"waiting","nodes":[]}]}
EOF
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" "$CWS" 109)
t "v2 runs come from dagu" "  v2 waiting at skill-werk-go · 3 runs" "$out"
t "a failed v2 run names the step dagu failed" "failed      skill-werk-commit" "$out"
t "a waiting v2 run names the step it waits at" "waiting     skill-werk-go" "$out"
if [[ "$out" == *skill-werk-build* ]]; then echo "FAIL another card's dagu run (CARD=1090) counted for 109"; fails=$((fails+1)); fi
t "the newest run waiting at the go prints the go" "human-task complete cicd --run-id r2 --step skill_werk_go --input accepter=jeff" "$out"
# Wren 2026-10-10: without --config the CLI answers "run not found"
t "the go names dagu's config" "go: dagu --config /x/platform/config/dagu.yaml human-task complete cicd" "$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" CWS_DAGU_CONFIG=/x/platform/config/dagu.yaml "$CWS" 109)"
t "an older run still waiting at the go is superseded" "superseded  skill-werk-go" "$out"
if [[ "$out" == *"--run-id r0"* ]]; then echo "FAIL a superseded run is offered a go"; fails=$((fails+1)); fi
t "dagu down says so, never silent" "v2 (dagu): unreadable" "$(CWS_RUNS_DIR=$TD "$CWS" 109)"
t "a v2-only card opens with its one line" "#109 kade   waiting for your go" "$out"
if [[ "$out" == *"no runs"* ]]; then echo "FAIL a v2-only card opens with the dead v1 line"; fails=$((fails+1)); fi

# #4494 — the land runs as its own workflow on the land queue; /cws follows it.
# NEGATIVE PROOF: another card's land (CARD=1090) is not this card's.
python3 - "$TD/dagu/api/v1/dag-runs" <<'EOF2'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
d["dagRuns"][:0] = [
 {"name":"cicd-land","dagRunId":"L2","params":"CARD=1090 ROLE=kade ACCEPTER=jeff","statusLabel":"failed","queuedAt":"2026-10-09T23:30:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-sync"}}]},
 {"name":"cicd-land","dagRunId":"L1","params":"CARD=109 ROLE=kade ACCEPTER=jeff","statusLabel":"running","queuedAt":"2026-10-09T23:00:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-merge"}},{"statusLabel":"running","step":{"name":"skill-werk-deploy-canonical"}}]}]
json.dump(d, open(p, "w"))
EOF2
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" "$CWS" 109)
t "the card's land shows on the land queue" "v2 land running at skill-werk-deploy-canonical · 1 land · dagu L1" "$out"
if [[ "$out" == *"dagu L2"* ]]; then echo "FAIL another card's land (CARD=1090) counted for 109"; fails=$((fails+1)); fi

# #4494 — the board view: one line per card, where it is now, which run.
# Jeff 2026-10-10: "the cws output is confusing".
mkdir -p "$TD/api/api/chorus/context/board"
echo '{"data":{"cards":[{"id":109,"owner":"Kade"},{"id":110,"owner":"Wren"}]}}' > "$TD/api/api/chorus/context/board/wip"
board() { CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" CWS_API="file://$TD/api" "$CWS"; }
out=$(board)
t "a landing card reads where its land is" "#109 kade   land: deploy-canonical" "$out"
t "its run number is the card's own count" "· run 3" "$out"
t "a card with no run says so plainly" "#110 wren   no run yet" "$out"
n=$(grep -c '^#109' <<<"$out")
[ "$n" = 1 ] || { echo "FAIL #109 takes $n lines, wanted one"; fails=$((fails+1)); }
if [[ "$out" == *"no pipeline run yet"* ]]; then echo "FAIL the dead v1 line is back"; fails=$((fails+1)); fi
# a queued land names its place in line, from dagu's queue
python3 - "$TD/dagu/api/v1/dag-runs" <<'EOF3'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
for r in d["dagRuns"]:
    if r["dagRunId"] == "L1": r["statusLabel"] = "queued"; r["nodes"] = []
json.dump(d, open(p, "w"))
EOF3
echo '{"items":[{"dagRunId":"Lx","statusLabel":"queued"},{"dagRunId":"L1","statusLabel":"queued"}]}' > "$TD/dagu/api/v1/queues/werk-land/items"
t "a queued land names its place in line" "#109 kade   land: waiting in line (2nd)" "$(board)"

rm -rf "$TD"
if [ $fails -gt 0 ]; then echo "test-cws-3782: $fails FAILURE(S)"; exit 1; fi
echo "test-cws-3782: all green"
