#!/usr/bin/env bats
# @test-type: unit
# @domain: tests
# @card: #4707
# @owner: silas
# #4707 reopen — platform/scripts/werk-land-retry retries a stopped land on the
# go Jeff already gave. On 10-23 #4715's land stopped at borg-ask while the
# roles door restarted, and the hook (rightly) refused every raw retry. What
# Jeff sees: a land that stopped after his go is finished without a second go,
# and a land nobody gave a go for (the 06:03 #4704 curl) is never retried.
# Fixtures are dagu run files in a scratch data dir; dagu itself is a fake that
# records what it was asked to run.

setup() {
  ROOT="$BATS_TEST_DIRNAME/../.."
  VERB="$ROOT/platform/scripts/werk-land-retry"
  export WLR_DATA="$BATS_TEST_TMPDIR/data" WLR_LOG_DIR="$BATS_TEST_TMPDIR/logs"
  export WLR_DAGU_BIN="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > %s/dagu-called\n' "$BATS_TEST_TMPDIR" > "$WLR_DAGU_BIN"
  chmod +x "$WLR_DAGU_BIN"
}

# run <dag> <id> <params> <status> <nodes: name=status,...> [go inputs json] [enqueued land id]
run_file() {
  python3 - "$WLR_DATA" "$BATS_TEST_TMPDIR" "$@" <<'PY'
import json, os, sys
data, tmp, dag, rid, params, status, nodes = sys.argv[1:8]
go = json.loads(sys.argv[8]) if len(sys.argv) > 8 and sys.argv[8] else None
enq = sys.argv[9] if len(sys.argv) > 9 else None
d = f"{data}/dag-runs/{dag}/dag-runs/2026/10/23/dag-run_20261023_221141Z_{rid}/a_20261023_221144Z_a1"
os.makedirs(d, exist_ok=True)
ns = []
for pair in nodes.split(","):
    name, st = pair.split("=")
    n = {"step": {"name": name}, "status": int(st)}
    if name == "skill-werk-go" and go is not None:
        n["humanTaskInput"] = go
    if name == "cicd-land" and enq:
        out = f"{tmp}/{rid}-cicd-land.out"
        open(out, "w").write(json.dumps({"msg": "Enqueued dag-run", "dag": "cicd-land", "run-id": enq}).replace(" ", "") + "\n")
        n["stdout"] = out
    ns.append(n)
open(f"{d}/status.jsonl", "w").write(json.dumps({"name": dag, "dagRunId": rid, "params": params,
    "status": int(status), "queuedAt": "2026-10-23T22:11:41Z", "nodes": ns}) + "\n")
PY
}

LAND_STOPPED="skill-werk-merge=4,skill-werk-sync=4,skill-werk-deploy-prod=4,skill-borg-ask=2,skill-werk-accept=3"
CICD_DONE="skill-werk-demo=4,skill-werk-go=4,cicd-land=4"

go_given() {  # the 4715 shape: Jeff's go enqueued land L1, which stopped at borg-ask
  local said='{"accepter":"jeff","decision":"go"}'
  [ $# -gt 0 ] && said="$1"
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" "$said" "${2:-L1}"
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 2 "$LAND_STOPPED"
}

@test "a land that stopped after Jeff's go is retried from its stopped step, on that go" {
  go_given
  run "$VERB" 4715
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"the go is cicd run C1 (accepter=jeff, decision=go)"* ]] || { echo "$output"; return 1; }
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -f "$BATS_TEST_TMPDIR/dagu-called" ] && break; sleep 0.2; done
  called=$(cat "$BATS_TEST_TMPDIR/dagu-called")
  [[ "$called" == *"retry --run-id=L1 --step=skill-borg-ask --downstream cicd-land" ]] || { echo "$called"; return 1; }
}

@test "negative proof: a land enqueued by hand (no go step names it) is refused, dagu untouched" {
  go_given '{"accepter":"jeff","decision":"go"}' L0   # the go enqueued a different run
  run "$VERB" 4715
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"no recorded go enqueued #4715's land L1"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "negative proof: a go step answered no, or by anyone but jeff, is not a go" {
  for said in '{"accepter":"jeff","decision":"no"}' '{"accepter":"kade","decision":"go"}' '{}'; do
    rm -rf "$WLR_DATA"
    go_given "$said"
    run "$VERB" 4715
    [ "$status" -eq 1 ] || { echo "$said: $output"; return 1; }
    [[ "$output" == *"no recorded go"* ]] || { echo "$said: $output"; return 1; }
  done
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "negative proof: a land with no ACCEPTER=jeff is refused" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=flow" 2 "$LAND_STOPPED"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"carries no ACCEPTER=jeff"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a land still running, or another land in flight, is not retried" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 1 "skill-werk-merge=4,skill-borg-ask=1"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"is running, not stopped"* ]] || { echo "$output"; return 1; }
  go_given
  run_file cicd-land L9 "CARD=4719 ROLE=abby-normal ACCEPTER=jeff" 1 "skill-werk-merge=1"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"lands go one at a time"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "a land that reads succeeded with steps never run is stopped, not done (#4540)" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 4 "skill-werk-merge=4,skill-borg-ask=4,skill-werk-accept=0"
  run "$VERB" 4715 --dry-run
  [ "$status" -eq 0 ] && [[ "$output" == *"--step=skill-werk-accept --downstream cicd-land"* ]] || { echo "$output"; return 1; }
}
