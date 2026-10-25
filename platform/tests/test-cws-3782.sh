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

# #4453 — every failed row names its reason. #4447 run 4 read "failed build
# (3s)" with an empty why; the refusal printed before the step began.
cat > "$TD/112.json" <<EOF
{"card":112,"role":"kade","phase":"failed","pid":999905,"startedAt":"$NOW","runId":"112-t-10"}
EOF
cat > "$TD/112-112-t-10.log" <<'EOF'
resume-refused: werk tree changed since the failed run — rerun from the start
[werk/werk] ⭐ Run Main build
[werk/werk]   ❌  Failure - Main build [3s]
WERK_EXIT=1
EOF
t "a refusal before the step is the failed row's why" "resume-refused: werk tree changed" "$(CWS_RUNS_DIR=$TD "$CWS" 112)"
cat > "$TD/114.json" <<EOF
{"card":114,"role":"kade","phase":"failed","pid":999906,"startedAt":"$NOW","runId":"114-t-11"}
EOF
printf '[werk/werk] ⭐ Run Main build\n[werk/werk]   ❌  Failure - Main build [3s]\nWERK_EXIT=1\n' > "$TD/114-114-t-11.log"
t "NEGATIVE PROOF: a log with no reason says so, never blank" "no reason in the run log" "$(CWS_RUNS_DIR=$TD "$CWS" 114)"
# #4453 — #4444: the header said running while the run's own log had ended.
cat > "$TD/113.json" <<EOF
{"card":113,"role":"kade","phase":"running","pid":$$,"startedAt":"$NOW","runId":"113-t-12"}
EOF
cat > "$TD/113-113-t-12.log" <<'EOF'
[werk/werk] ⭐ Run Main demo
[werk/werk]   ✅  Success - Main demo [6s]
WERK_EXIT=0
EOF
out=$(CWS_RUNS_DIR=$TD "$CWS" 113)
t "a pin left saying running after its log ended reads the log" "#113 [kade] presented (the pin still says running" "$out"
if [[ "$(head -1 <<<"$out")" == *"] running"* ]]; then echo "FAIL the header still says running"; fails=$((fails+1)); fi

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
# #4494 reopen — #4004 read "[?] landed · 18 runs" from its old v1 pin while
# its v2 land ran. A card with v1 history AND v2 runs opens with the v2 line.
# NEGATIVE PROOF: 109 gets a v1 pin + log saying landed; the header stays v2.
echo "{\"card\":109,\"role\":\"kade\",\"phase\":\"landed\",\"pid\":999909,\"startedAt\":\"$NOW\",\"runId\":\"109-t-13\"}" > "$TD/109.json"
printf '[werk/werk] ⭐ Run Main land\n[werk/werk]   ✅  Success - Main land [6s]\nWERK_EXIT=0\n' > "$TD/109-109-t-13.log"
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" "$CWS" 109)
t "a card with v1 history still opens with its v2 line" "#109 kade   waiting for your go" "$out"
if [[ "$(head -1 <<<"$out")" == *"[kade] landed"* ]]; then echo "FAIL the v1 pin's landed is the header"; fails=$((fails+1)); fi
# #4494 reopen 2 — Jeff 2026-10-13 (via Wren): "/cws looks broken". The per-card
# header read "run 3 · 1 run" (v2's count beside v1's), and the go/no commands
# rode the header AND the v2 line. The header says the v1 runs are the old
# pipeline's, and the commands print once, under the v2 runs.
hd="$(head -1 <<<"$out")"
t "the per-card header names the old pipeline's runs as such" "· 1 old-pipeline run" "$hd"
if [[ "$hd" =~ "run 3 · 1 run"$ ]]; then echo "FAIL the header mixes v2's run count with v1's"; fails=$((fails+1)); fi
n=$(grep -o "human-task complete" <<<"$out" | wc -l | tr -d ' ')
[ "$n" = 2 ] || { echo "FAIL the go/no commands print $n times on /cws 109, wanted once each"; fails=$((fails+1)); }
# The board view: a card waiting at the go reads its status only; the commands
# live on /cws <card>. NEGATIVE PROOF: the line must not carry human-task.
mkdir -p "$TD/apiw/api/chorus/context/board"
echo '{"data":{"cards":[{"id":109,"owner":"Kade"}]}}' > "$TD/apiw/api/chorus/context/board/wip"
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" CWS_API="file://$TD/apiw" "$CWS")
t "board: a card waiting at the go reads its status" "#109 kade   waiting for your go" "$out"
if [[ "$out" == *"human-task"* ]]; then echo "FAIL the board line carries the go/no commands"; fails=$((fails+1)); fi
rm -f "$TD/109.json" "$TD/109-109-t-13.log"

# #4494 — the land runs as its own workflow on the land queue; /cws follows it.
# NEGATIVE PROOF: another card's land (CARD=1090) is not this card's.
python3 - "$TD/dagu/api/v1/dag-runs" <<'EOF2'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
d["dagRuns"][:0] = [
 {"name":"cicd-land","dagRunId":"L2","params":"CARD=1090 ROLE=kade ACCEPTER=jeff","statusLabel":"failed","queuedAt":"2026-10-09T23:30:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-sync"}}]},
 {"name":"cicd-land","dagRunId":"L1","params":"CARD=109 ROLE=kade ACCEPTER=jeff","statusLabel":"running","queuedAt":"2026-10-09T23:00:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-merge"}},{"statusLabel":"running","step":{"name":"skill-werk-deploy-prod"}}]}]
json.dump(d, open(p, "w"))
EOF2
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" "$CWS" 109)
# #4734 — the land is a row of the table, like every other pipeline's run
t "the card's land shows on the land queue" "running     skill-werk-deploy-prod" "$out"
t "…as a cicd-land row" "  cicd-land    1  " "$out"
if [[ "$out" == *"dagu L2"* ]]; then echo "FAIL another card's land (CARD=1090) counted for 109"; fails=$((fails+1)); fi

# #4494 — a go can be a no; the go line offers both, and a no reads "rejected"
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" "$CWS" 109)
t "the go line offers the no" "--input decision=no" "$out"
t "the go itself says go" "--input decision=go" "$out"

# #4494 — the board view: one line per card, where it is now, which run.
# Jeff 2026-10-10: "the cws output is confusing".
mkdir -p "$TD/api/api/chorus/context/board"
echo '{"data":{"cards":[{"id":109,"owner":"Kade"},{"id":110,"owner":"Wren"}]}}' > "$TD/api/api/chorus/context/board/wip"
board() { CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" CWS_API="file://$TD/api" "$CWS"; }
out=$(board)
t "a landing card reads where its land is" "#109 kade   land: deploy-prod" "$out"
t "its run number is the card's own count" "· cicd run 3" "$out"
t "a card with no run says so plainly" "#110 wren   no run yet" "$out"
n=$(grep -c '^#109' <<<"$out")
[ "$n" = 1 ] || { echo "FAIL #109 takes $n lines, wanted one"; fails=$((fails+1)); }
if [[ "$out" == *"no pipeline run yet"* ]]; then echo "FAIL the dead v1 line is back"; fails=$((fails+1)); fi
# #4494 reopen — Jeff 2026-10-12: "#4004 silas build" while Kade built it. The
# line names the role whose run it is (the run's ROLE), not the board owner.
# NEGATIVE PROOF: card 112 is Wren's on the board, its run is Kade's.
echo '{"data":{"cards":[{"id":109,"owner":"Kade"},{"id":110,"owner":"Wren"},{"id":112,"owner":"Wren"}]}}' > "$TD/api/api/chorus/context/board/wip"
python3 - "$TD/dagu/api/v1/dag-runs" <<'EOF5'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
d["dagRuns"].append({"name":"cicd","dagRunId":"r12","params":"CARD=112 ROLE=kade","statusLabel":"running","startedAt":"2026-10-09T23:10:00-04:00","nodes":[{"statusLabel":"running","step":{"name":"skill-werk-build"}}]})
json.dump(d, open(p, "w"))
EOF5
out=$(board)
t "a run names the role that runs it, not the card's owner" "#112 kade   build" "$out"
if [[ "$out" == *"#112 wren"* ]]; then echo "FAIL #112 reads the board owner"; fails=$((fails+1)); fi
t "a card with no run still names its owner" "#110 wren   no run yet" "$out"
roled() { CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/dagu" CWS_API="file://$TD/api" "$CWS" --role "$1"; }
t "--role follows the run's role" "#112 kade" "$(roled kade)"
if [[ "$(roled wren)" == *"#112"* ]]; then echo "FAIL --role wren shows Kade's run of #112"; fails=$((fails+1)); fi
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
# a no at the go: the land was skipped and the run ended (no land row for it)
python3 - "$TD/dagu/api/v1/dag-runs" <<'EOF4'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
d["dagRuns"] = [r for r in d["dagRuns"] if r.get("name") != "cicd-land"]
for r in d["dagRuns"]:
    if r["dagRunId"] == "r2":
        r["statusLabel"] = "succeeded"
        r["nodes"] = [{"statusLabel":"succeeded","step":{"name":"skill-werk-go"}},{"statusLabel":"skipped","step":{"name":"cicd-land"}}]
json.dump(d, open(p, "w"))
EOF4
out=$(board)
t "a no at the go reads rejected" "#109 kade   rejected at the go" "$out"
if [[ "$out" == *"#109 kade   landed"* ]]; then echo "FAIL a rejected run reads landed"; fails=$((fails+1)); fi

# #4453 — a card run's tests, from the run's own events (Loki; a file:// dir
# holding <trace>.jsonl stands in). NEGATIVE PROOF: another run's failed case
# (trace r9, card 1110) never shows on card 111.
mkdir -p "$TD/d2/api/v1" "$TD/loki" "$TD/api2/api/chorus/context/board"
cat > "$TD/d2/api/v1/dag-runs" <<'EOF5'
{"dagRuns":[
 {"name":"cicd","dagRunId":"r9","params":"CARD=1110 ROLE=wren","statusLabel":"failed","startedAt":"2026-10-11T06:00:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-test"}}]},
 {"name":"cicd","dagRunId":"r5","params":"CARD=111 ROLE=kade","statusLabel":"running","startedAt":"2026-10-11T07:00:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-build"},"startedAt":"2026-10-11T07:00:00-04:00"},{"statusLabel":"running","step":{"name":"skill-werk-test"},"startedAt":"2026-10-11T07:01:00-04:00"}]}
]}
EOF5
cat > "$TD/loki/r5.jsonl" <<'EOF5'
{"timestamp":"2026-10-11T07:01:00","event":"test.started","checks_planned":"4"}
{"timestamp":"2026-10-11T07:01:01","event":"test.unit.started","check":"tsc","unit":"platform/api"}
{"timestamp":"2026-10-11T07:01:09","event":"test.unit.timed","check":"tsc","unit":"platform/api","verdict":"ok","seconds":"8.0"}
{"timestamp":"2026-10-11T07:01:10","event":"test.unit.started","check":"jest","unit":"platform/api"}
{"timestamp":"2026-10-11T07:02:10","event":"test.case.failed","unit":"platform/api","file":"platform/api/tests/tree.test.ts","case":"the tree answers from the graph","reason":"expected 10 got 0"}
{"timestamp":"2026-10-11T07:02:11","event":"test.unit.timed","check":"jest","unit":"platform/api","verdict":"FAIL","seconds":"61.0"}
{"timestamp":"2026-10-11T07:02:12","event":"test.unit.started","check":"cargo-test","unit":"chorus-hooks"}
{"timestamp":"2026-10-11T07:02:30","event":"test.unit.progress","check":"cargo-test","unit":"chorus-hooks","message":"cargo-test chorus-hooks: 120 passed · 0 failed of 476"}
EOF5
cat > "$TD/loki/r9.jsonl" <<'EOF5'
{"timestamp":"2026-10-11T06:01:00","event":"test.unit.started","check":"jest","unit":"platform/api"}
{"timestamp":"2026-10-11T06:02:00","event":"test.case.failed","unit":"platform/api","file":"platform/api/tests/other.test.ts","case":"ANOTHER CARD'S CASE","reason":"x"}
EOF5
cws2() { CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d2" CWS_LOKI="file://$TD/loki" "$CWS" "$@"; }
out=$(cws2 111)
t "the run's tests: done of planned, ok, failed, running" "tests (cicd run 1): 1 ok · 1 failed · running (2 of 4)" "$out"
t "a failed suite names itself" "✗ jest:platform/api" "$out"
t "its failed case sits under it, with the reason" "tree.test.ts — the tree answers from the graph — expected 10 got 0" "$out"
t "a long unit shows its count while it runs" "… cargo-test:chorus-hooks running — 120 passed · 0 failed of 476" "$out"
if [[ "$out" == *"ANOTHER CARD'S CASE"* ]]; then echo "FAIL another run's case shows on #111"; fails=$((fails+1)); fi
t "Loki down says so, never silent" "unreadable — Loki did not answer" "$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d2" CWS_LOKI="http://127.0.0.1:9" "$CWS" 111)"
echo '{"data":{"cards":[{"id":111,"owner":"Kade"}]}}' > "$TD/api2/api/chorus/context/board/wip"
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d2" CWS_LOKI="file://$TD/loki" CWS_API="file://$TD/api2" "$CWS")
t "the board line carries the run's tests" "#111 kade   test" "$out"
t "…as one short summary" "· tests 1 ok · 1 failed · running (2 of 4)" "$out"

# #4186 reopen — #4513's model failed at 12:05 on 2026-10-13 and the card read
# "landed"; only the land's success handler log said so. The handler's result
# line rides the card's own line, and a landed card (off WIP) whose model failed
# stays on the board view. NEGATIVE PROOF: a deployed model adds nothing.
mkdir -p "$TD/d3/api/v1" "$TD/api3/api/chorus/context/board"
echo 'athena-land #120: athena-deploy land did not finish clean
athena-land #120: result=failed' > "$TD/h120.out"
echo 'athena-land #121: result=deployed' > "$TD/h121.out"
cat > "$TD/d3/api/v1/dag-runs" <<EOF6
{"dagRuns":[
 {"name":"cicd","dagRunId":"c120","params":"CARD=120 ROLE=wren","statusLabel":"succeeded","startedAt":"2026-10-13T11:58:00-04:00","finishedAt":"2026-10-13T12:03:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-go"}},{"statusLabel":"succeeded","step":{"name":"cicd-land"}}]},
 {"name":"cicd-land","dagRunId":"A120","params":"CARD=120 ROLE=wren ACCEPTER=jeff","statusLabel":"succeeded","queuedAt":"2026-10-13T12:03:10-04:00","startedAt":"2026-10-13T12:03:20-04:00","finishedAt":"2026-10-13T12:05:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-accept"}}],"onSuccess":{"statusLabel":"failed","stdout":"$TD/h120.out"}},
 {"name":"cicd","dagRunId":"c121","params":"CARD=121 ROLE=kade","statusLabel":"succeeded","startedAt":"2026-10-13T11:50:00-04:00","finishedAt":"2026-10-13T11:52:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"cicd-land"}}]},
 {"name":"cicd-land","dagRunId":"A121","params":"CARD=121 ROLE=kade ACCEPTER=jeff","statusLabel":"succeeded","queuedAt":"2026-10-13T11:52:10-04:00","startedAt":"2026-10-13T11:52:20-04:00","finishedAt":"2026-10-13T11:54:00-04:00","nodes":[{"statusLabel":"succeeded","step":{"name":"skill-werk-accept"}}],"onSuccess":{"statusLabel":"succeeded","stdout":"$TD/h121.out"}}
]}
EOF6
echo '{"data":{"cards":[]}}' > "$TD/api3/api/chorus/context/board/wip"
cws3() { CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d3" CWS_API="file://$TD/api3" "$CWS" "$@"; }
t "a land whose model failed says so on the card's line" "#120 wren   landed · athena failed" "$(cws3 120)"
t "…and under the land rows" "athena failed · land A120" "$(cws3 120)"
out=$(cws3)
t "board: a landed card whose model failed stays on the board" "#120 wren   landed · athena failed" "$out"
if [[ "$out" == *"#121"* ]]; then echo "FAIL a land whose model deployed is on the board"; fails=$((fails+1)); fi
out=$(cws3 121)
t "a land whose model deployed reads landed" "#121 kade   landed " "$out"
if [[ "$(head -1 <<<"$out")" == *"athena failed"* ]]; then echo "FAIL a deployed model reads failed"; fails=$((fails+1)); fi

# #4494 reopen 2026-10-14 — dagu pages its run list (100 a page, nextCursor);
# cws read page 1 only, so #4474 counted 3 runs of 25+. Two pages, two runs of
# 130 on each: the count is 2 only if page 2 is read. NEGATIVE PROOF: the same
# card with page 1 alone (no cursor) counts 1.
mkdir -p "$TD/d4/api/v1"
cat > "$TD/d4/api/v1/dag-runs" <<'EOF7'
{"dagRuns":[{"name":"cicd","dagRunId":"p1","params":"CARD=130 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-14T08:00:00-04:00","finishedAt":"2026-10-14T08:01:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-build"}}]}],"nextCursor":"pg2"}
EOF7
cat > "$TD/d4/api/v1/dag-runs@pg2" <<'EOF7'
{"dagRuns":[{"name":"cicd","dagRunId":"p0","params":"CARD=130 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-13T08:00:00-04:00","finishedAt":"2026-10-13T08:01:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-build"}}]}]}
EOF7
t "runs on dagu's second page count" "· 2 runs" "$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d4" "$CWS" 130)"
mkdir -p "$TD/d5/api/v1"; python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d.pop("nextCursor"); json.dump(d,open(sys.argv[2],"w"))' "$TD/d4/api/v1/dag-runs" "$TD/d5/api/v1/dag-runs"
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d5" "$CWS" 130)
if [[ "$out" == *"· 2 runs"* ]]; then echo "FAIL page 1 alone counted the second page's run"; fails=$((fails+1)); fi

# #4536 — Jeff 2026-10-15 13:10 "that cws readout is wrong": #4536 had 8 athena
# runs and /cws said "no runs · 0 runs". A card's run starts on the athena
# pipeline, so its athena runs are its runs.
mkdir -p "$TD/d6/api/v1"
cat > "$TD/d6/api/v1/dag-runs" <<'EOF8'
{"dagRuns":[{"name":"athena","dagRunId":"a2","params":"CARD=140 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T12:04:00-04:00","finishedAt":"2026-10-15T12:10:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-athena-validate-validate"}}]},{"name":"athena","dagRunId":"a1","params":"CARD=140 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T11:04:00-04:00","finishedAt":"2026-10-15T11:05:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-athena-gen"}}]},{"name":"athena","dagRunId":"x1","params":"CARD=1400 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T11:00:00-04:00","finishedAt":"2026-10-15T11:01:00-04:00","nodes":[]},{"name":"cicd","dagRunId":"c1","params":"CARD=140 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T12:20:00-04:00","finishedAt":"2026-10-15T12:21:00-04:00","nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-build"}}]}]}
EOF8
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d6" "$CWS" 140)
t "athena runs count as the card's runs" "· 3 runs" "$out"
t "the newest run of either half leads" "failed at skill-werk-build" "$out"
# NEGATIVE PROOF: with only cicd read, the athena runs vanish (the 13:10 readout)
out=$(CWS_RUNS_DIR=$TD CWS_DAGU="file://$TD/d6" CWS_ATHENA_DAG=cicd "$CWS" 140)
if [[ "$out" == *"· 3 runs"* ]]; then echo "FAIL athena runs counted with athena not read"; fails=$((fails+1)); fi

# #4553 (Kade) — a second run of a card refuses at its first step while another
# is live, naming that run; #4536 run 14 died 3 minutes in on the build lock.
# NEGATIVE PROOF: the run asking (me) and a run waiting at an old go do not count.
mkdir -p "$TD/d7/api/v1"
cat > "$TD/d7/api/v1/dag-runs" <<'EOF9'
{"dagRuns":[{"name":"cicd","dagRunId":"me","params":"CARD=160 ROLE=kade","statusLabel":"running","startedAt":"2026-10-15T15:38:00-04:00","nodes":[]},{"name":"cicd","dagRunId":"busy","params":"CARD=160 ROLE=kade","statusLabel":"running","startedAt":"2026-10-15T15:35:00-04:00","nodes":[]},{"name":"cicd","dagRunId":"old","params":"CARD=160 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T15:00:00-04:00","nodes":[]},{"name":"cicd","dagRunId":"me2","params":"CARD=161 ROLE=kade","statusLabel":"running","startedAt":"2026-10-15T15:38:00-04:00","nodes":[]},{"name":"cicd","dagRunId":"go","params":"CARD=161 ROLE=kade","statusLabel":"waiting","startedAt":"2026-10-15T15:00:00-04:00","nodes":[]}]}
EOF9
out=$(CWS_DAGU="file://$TD/d7" "$CWS" --live-other 160 me); rc=$?
t "another live run refuses, naming it" "refused: cicd run 2 is live (dagu busy)" "$out"
[ $rc -eq 3 ] || { echo "FAIL live-other exit=$rc, wanted 3"; fails=$((fails+1)); }
CWS_DAGU="file://$TD/d7" "$CWS" --live-other 161 me2 >/dev/null; rc=$?
[ $rc -eq 0 ] || { echo "FAIL a run waiting at an old go blocked a new run (exit=$rc)"; fails=$((fails+1)); }
out=$(CWS_DAGU="http://127.0.0.1:9" "$CWS" --live-other 160 me); rc=$?
t "dagu unreadable says so" "dagu unreadable" "$out"
[ $rc -eq 2 ] || { echo "FAIL unreadable dagu exit=$rc, wanted 2"; fails=$((fails+1)); }

# #4553 (Kade) — the failure nudge's words: a refused run says "refused: … is
# live", never "failed at commit". NEGATIVE PROOF: a real commit failure names
# its step; an unknown run exits 2 so the handler keeps its plain words.
mkdir -p "$TD/d8/api/v1/dag-runs/cicd"
echo '{"dagRunDetails":{"nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-commit"},"error":"exit status 1\nwerk-commit: refused: cicd run 13 is live (dagu busy)"}]}}' > "$TD/d8/api/v1/dag-runs/cicd/r1"
echo '{"dagRunDetails":{"nodes":[{"statusLabel":"failed","step":{"name":"skill-werk-commit"},"error":"exit status 1\nwerk-commit: commit failed (pre-commit gate?)"}]}}' > "$TD/d8/api/v1/dag-runs/cicd/r2"
out=$(CWS_DAGU="file://$TD/d8" "$CWS" --why cicd r1)
t "a refused run's nudge says refused" "refused: cicd run 13 is live (dagu busy)" "$out"
if [[ "$out" == *"failed at"* ]]; then echo "FAIL a refused run reads failed"; fails=$((fails+1)); fi
t "a commit failure's nudge names its step" "cicd failed at commit: werk-commit: commit failed" "$(CWS_DAGU="file://$TD/d8" "$CWS" --why cicd r2)"
CWS_DAGU="file://$TD/d8" "$CWS" --why cicd nope >/dev/null; rc=$?
[ $rc -eq 2 ] || { echo "FAIL unknown run exit=$rc, wanted 2"; fails=$((fails+1)); }

rm -rf "$TD"
if [ $fails -gt 0 ]; then echo "test-cws-3782: $fails FAILURE(S)"; exit 1; fi
echo "test-cws-3782: all green"
