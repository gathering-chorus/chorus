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
