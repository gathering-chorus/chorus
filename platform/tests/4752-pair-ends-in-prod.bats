#!/usr/bin/env bats
# @test-type: unit — fixture dagu nodes and step logs, no live dagu
# @domain: cicd
# @card: 4752 · owner: kade
# What Jeff sees: a failed pair or cicd-test run says WHY in /cws and in the
# failure nudge. #4814 (10-31) read "no reason in dagu's record of the step" for
# pair runs 2–4 and cicd-test runs 1–3, while the verdict step's own output said
# "werk-test: UNMEASURED — platform/tests/ac2-5-deploy-daemon-card-main.bats — not built".

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CWS="$REPO/platform/scripts/chorus-werk-status"

cws_py() {
  python3 - "$CWS" "$1" <<'EOF'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("cws", sys.argv[1])
spec = importlib.util.spec_from_loader("cws", loader)
cws = importlib.util.module_from_spec(spec)
loader.exec_module(cws)
exec(sys.argv[2])
EOF
}

# the pair script against a fake board, spine and dagu (as 4577-pair-steps.bats)
pair_env() {
  PAIR="$REPO/platform/scripts/pair"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" PAIR_DIR="$BATS_TEST_TMPDIR/pairs"
  S="$CHORUS_HOME/platform/scripts"; L="$BATS_TEST_TMPDIR/log"; REG="$CHORUS_HOME/ops/active-pairs"
  mkdir -p "$S" "$PAIR_DIR" "$L"
  export STATUS=WIP L
  cat > "$S/cards" <<'EOF'
case "$1" in
view) printf '#%s Pair it\n  Status:   %s\n  Desc:\n    ## AC\n    - [x] one\n    - [ ] two\n  Domains:  x\n' "$2" "$STATUS" ;;
comment) echo "$2|$3" >> "$L/comments" ;;
esac
EOF
  echo 'echo "$*" >> "$L/spine"' > "$S/chorus-log"
  echo 'echo "$1|$2" >> "$L/nudges"' > "$S/ops-nudge"
  printf '#!/usr/bin/env python3\nimport sys\nprint("why " + " ".join(sys.argv[2:4]))\n' > "$S/chorus-werk-status"
  printf '#!/usr/bin/env bash\necho "$*" >> %s/dagu\n' "$L" > "$BATS_TEST_TMPDIR/dagu"; chmod +x "$BATS_TEST_TMPDIR/dagu"
  export PAIR_DAGU="$BATS_TEST_TMPDIR/dagu" DAG_RUN_ID=run1
  D="$BATS_TEST_TMPDIR/door/identity/principals"; mkdir -p "$D"
  for r in kade silas; do printf '{"data":{"hostAccount":"%s"}}' "$(id -un)" > "$D/$r"; done
  export PAIR_DOOR="file://$BATS_TEST_TMPDIR/door"
  unset CHORUS_ROLE
}

@test "AC1: the hand-off keeps the pair registered, marked handed off, with no For-Jeff yet" {
  pair_env
  bash "$PAIR" start 4752 kade silas >/dev/null
  run bash "$PAIR" handoff 4752 kade silas
  echo "$output"
  [ "$status" -eq 0 ]
  grep -q '"phase":"handed-off"' "$REG/4752.json"
  grep -q '"handed_off_epoch":"[0-9]' "$REG/4752.json"
  ! grep -q '## For Jeff' "$PAIR_DIR/pair-4752.md" || return 1
  grep -q '^pair.handed_off system card=4752' "$L/spine"
  ! grep -q 'pair.ended' "$L/spine" || return 1
}

@test "AC2: a cicd or land red on a paired card goes back to the same pair: the navigator re-greens a new attempt" {
  pair_env
  bash "$PAIR" start 4752 kade silas >/dev/null
  bash "$PAIR" handoff 4752 kade silas >/dev/null
  run bash "$PAIR" routed 4752 cicd cicd-run-7
  echo "$output"
  [ "$status" -eq 0 ]
  grep -q 'enqueue pair -- CARD=4752 ROLE=kade NAVIGATOR=silas' "$L/dagu"
  grep -q '^silas|pair #4752 red: why cicd cicd-run-7' "$L/nudges"
  # AC5 — the red runs no second cicd-test: the next test is cicd's, after the green
  ! grep -q 'cicd-test' "$L/dagu" || return 1
  # the new attempt is the same pair: back to slice, not a second pair.started
  bash "$PAIR" start 4752 kade silas >/dev/null
  grep -q '^## Back to slice' "$PAIR_DIR/pair-4752.md"
  [ "$(grep -c pair.started "$L/spine")" -eq 1 ]
  grep -q '"phase":"slicing"' "$REG/4752.json"
}

@test "AC2 negative proof: a red on a card with no pair is not routed (exit 1, the builder's nudge runs), and enqueues nothing" {
  pair_env
  run bash "$PAIR" routed 4752 cicd cicd-run-7
  [ "$status" -eq 1 ]
  [ ! -e "$L/dagu" ]
}

@test "AC1: the land proven in prod ends the pair: For-Jeff written, deregistered, pair.ended" {
  pair_env
  bash "$PAIR" start 4752 kade silas >/dev/null
  bash "$PAIR" handoff 4752 kade silas >/dev/null
  run bash "$PAIR" proven 4752
  echo "$output"
  [ "$status" -eq 0 ]
  [ ! -e "$REG/4752.json" ]
  grep -q '## For Jeff' "$PAIR_DIR/pair-4752.md"
  grep -q '^pair.ended system card=4752 driver=kade partner=silas' "$L/spine"
}

@test "AC1 negative proof: a land with no pair ends nothing and never fails the land" {
  pair_env
  run bash "$PAIR" proven 4752
  [ "$status" -eq 0 ]
  [ ! -e "$L/spine" ] || ! grep -q pair.ended "$L/spine"
}

@test "AC10: a stopped pair's later cicd red runs no pair red" {
  pair_env
  bash "$PAIR" start 4752 kade silas >/dev/null
  bash "$PAIR" check 4752 silas silas stop >/dev/null 2>&1 || true
  run bash "$PAIR" routed 4752 cicd cicd-run-8
  [ "$status" -eq 1 ]
  [ ! -e "$L/dagu" ]
}

@test "AC10: a red on a paired card that left WIP ends the pair instead of re-enqueueing" {
  pair_env
  bash "$PAIR" start 4752 kade silas >/dev/null
  bash "$PAIR" handoff 4752 kade silas >/dev/null
  STATUS=Done run bash "$PAIR" routed 4752 cicd-land land-run-2
  [ "$status" -eq 1 ]
  [ ! -e "$L/dagu" ]
  [ ! -e "$REG/4752.json" ]
  grep -q '^pair.ended system card=4752' "$L/spine"
}

# the verdict step's stdout as #4814's cicd-test run 3 wrote it
verdict_log() {
  cat >"$BATS_TEST_TMPDIR/verdict.out" <<'EOF'
werk-test: running the run's pinned build /x/werk-test-bin
   UNMEASURED: 1 suite(s) scored nothing — platform/tests/ac2-5-deploy-daemon-card-main.bats — not built
werk-test: UNMEASURED — platform/tests/ac2-5-deploy-daemon-card-main.bats — not built (exit 2)
EOF
  : >"$BATS_TEST_TMPDIR/verdict.err"
}

@test "AC9: a step whose stderr holds only the exit status reads its reason from its own stdout log" {
  verdict_log
  run cws_py "print(cws.step_reason({'error': 'exit status 2', 'stdout': '$BATS_TEST_TMPDIR/verdict.out', 'stderr': '$BATS_TEST_TMPDIR/verdict.err'}))"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == "werk-test: UNMEASURED — platform/tests/ac2-5-deploy-daemon-card-main.bats"* ]] || return 1
}

@test "AC9 negative proof: a step with nothing in either record still says so, never an invented reason" {
  : >"$BATS_TEST_TMPDIR/empty.out"
  run cws_py "print(cws.step_reason({'error': 'exit status 2', 'stdout': '$BATS_TEST_TMPDIR/empty.out'}), cws.step_reason({'error': 'exit status 1', 'stdout': '/no/such/file'}))"
  echo "$output"
  [ "$output" = "no reason in dagu's record of the step no reason in dagu's record of the step" ]
}

@test "AC9: the pair's local proof step finds the cicd-test run it enqueued (TRACE = the pair run) and gives that run's reason" {
  verdict_log
  run cws_py "
cws.STARTED[('cicd-test', '4814')] = [
  {'name': 'cicd-test', 'dagRunId': 'ct-other', 'params': 'CARD=4814 ROLE=silas SCOPE=diff TRACE=someone-else'},
  {'name': 'cicd-test', 'dagRunId': 'ct-3', 'params': 'CARD=4814 ROLE=silas SCOPE=diff TRACE=pair-4',
   'nodes': [{'step': {'name': 'skill-werk-test-verdict'}, 'statusLabel': 'failed', 'error': 'exit status 2', 'stdout': '$BATS_TEST_TMPDIR/verdict.out'}]}]
cws.nodes_of = lambda r: r.get('nodes', [])
run = {'name': 'pair', 'dagRunId': 'pair-4', 'params': 'CARD=4814 ROLE=silas NAVIGATOR=kade'}
node = {'step': {'name': 'skill-pair-local-proof', 'command': '\${CHORUS_HOME}/platform/scripts/dagu-enqueue-wait cicd-test CARD=\${CARD} ROLE=\${ROLE} TRACE=\${CHORUS_TRACE_ID} SCOPE=diff'}, 'statusLabel': 'failed', 'error': 'exit status 1'}
print(cws.failed_reason(run, node))"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == "test-verdict: werk-test: UNMEASURED — platform/tests/ac2-5-deploy-daemon-card-main.bats"* ]] || return 1
}

@test "AC9: the same, from dagu's own record of the step (command and args split, as #4814 pair run 4 holds it)" {
  verdict_log
  run cws_py "
cws.STARTED[('cicd-test', '4814')] = [{'name': 'cicd-test', 'dagRunId': 'ct-3', 'params': 'CARD=4814 SCOPE=diff TRACE=pair-4',
   'nodes': [{'step': {'name': 'skill-werk-test-verdict'}, 'statusLabel': 'failed', 'error': 'exit status 2', 'stdout': '$BATS_TEST_TMPDIR/verdict.out'}]}]
cws.nodes_of = lambda r: r.get('nodes', [])
run = {'name': 'pair', 'dagRunId': 'pair-4', 'params': 'CARD=4814 ROLE=silas NAVIGATOR=kade'}
node = {'step': {'name': 'skill-pair-local-proof', 'commands': [{'args': ['cicd-test', 'CARD=4814', 'TRACE=pair-4', 'SCOPE=diff'], 'command': '/x/platform/scripts/dagu-enqueue-wait'}]}, 'statusLabel': 'failed', 'error': 'exit status 1'}
print(cws.failed_reason(run, node))"
  echo "$output"
  [[ "$output" == "test-verdict: werk-test: UNMEASURED"* ]] || return 1
}

@test "AC9: a queued child's retry is the whole step, never 'only <inner step> runs again'" {
  verdict_log
  run cws_py "
cws.STARTED[('cicd-test', '4814')] = [{'name': 'cicd-test', 'dagRunId': 'ct-3', 'params': 'CARD=4814 TRACE=pair-4',
   'nodes': [{'step': {'name': 'skill-werk-test-verdict'}, 'statusLabel': 'failed', 'error': 'exit status 2'}]}]
cws.nodes_of = lambda r: r.get('nodes', [])
run = {'name': 'pair', 'dagRunId': 'pair-4', 'params': 'CARD=4814'}
node = {'step': {'name': 'skill-pair-local-proof', 'commands': [{'args': ['cicd-test', 'TRACE=pair-4'], 'command': '/x/dagu-enqueue-wait'}]}, 'statusLabel': 'failed'}
print(cws.retry_line('pair', 'pair-4', node, [node], run))"
  echo "$output"
  [[ "$output" == "retry: dagu "*"--step=skill-pair-local-proof --downstream pair" ]] || return 1
  [[ "$output" != *"runs again"* ]] || return 1
}

@test "AC9 negative proof: an enqueued cicd-test run of another pair run is never read as this one's" {
  run cws_py "
cws.STARTED[('cicd-test', '4814')] = [{'name': 'cicd-test', 'dagRunId': 'ct-other', 'params': 'CARD=4814 TRACE=someone-else',
   'nodes': [{'step': {'name': 'skill-werk-test-verdict'}, 'statusLabel': 'failed', 'error': 'FAIL: not mine'}]}]
cws.nodes_of = lambda r: r.get('nodes', [])
run = {'name': 'pair', 'dagRunId': 'pair-4', 'params': 'CARD=4814'}
node = {'step': {'name': 'skill-pair-local-proof', 'command': 'dagu-enqueue-wait cicd-test CARD=\${CARD}'}, 'statusLabel': 'failed', 'error': 'exit status 1'}
print(cws.failed_reason(run, node))"
  echo "$output"
  [[ "$output" != *"not mine"* ]] || return 1
}

