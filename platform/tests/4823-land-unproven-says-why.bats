#!/usr/bin/env bats
# @test-type: unit — fixture dagu nodes and step logs, no live dagu
# @domain: tests
# @card: 4823 · owner: kade
# What Jeff sees: a land that stops at the prod proof says, in one line, which
# AC is not proven and why. #4772 (10-31 06:26) and #4752 (10-31 20:37) both
# read "failed at borg-verify-in-prod: no reason in dagu's record of the step",
# while the step's own output named each AC and its reason.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CWS="$REPO/platform/scripts/chorus-werk-status"

cws_py() {
  python3 - "$CWS" "$1" <<'PY'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("cws", sys.argv[1])
spec = importlib.util.spec_from_loader("cws", loader)
cws = importlib.util.module_from_spec(spec)
loader.exec_module(cws)
exec(sys.argv[2])
PY
}

# borg-prove prod's output, as #4752's land wrote it (two AC not met)
prove_log() {
  cat >"$BATS_TEST_TMPDIR/prove.out" <<'OUT'
#4752 against http://localhost:3340
ac-4752-6    met        AC6: A red before any fix commit is not counted as a pair round
             test platform/tests/4752-pair-ends-in-prod.bats → pass
ac-4752-7    unmeasured AC7: A cicd test step on a werk with a package dir lacking node_modules fails once
             test platform/tests/werk-node-modules.bats: no result yet (no case event from this card's runs, nor from main's nightly since the land)
ac-4752-9    met        AC9: A failed pair or cicd-test run records its reason in dagu
             test platform/tests/4752-pair-ends-in-prod.bats → pass
#4752 verify in prod: Unproven
OUT
}

node() { # the failed land step, pointing at the log
  echo "{'step': {'name': 'skill-borg-verify-in-prod'}, 'statusLabel': 'failed', 'error': 'exit status 1', 'stdout': '$BATS_TEST_TMPDIR/prove.out'}"
}

@test "AC1/AC4: an unproven land names each AC that is not met and its reason, in one line" {
  prove_log
  run cws_py "print(cws.failed_reason({'name': 'cicd-land', 'dagRunId': 'l1', 'params': 'CARD=4752'}, $(node)))"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 1 ]
  [[ "$output" == "Unproven: ac-4752-7 unmeasured (werk-node-modules.bats: no result yet"* ]] || return 1
  [[ "$output" != *"ac-4752-6"* ]] || return 1
  [[ "$output" != *"no reason in dagu"* ]] || return 1
}

@test "AC3 negative proof: an unproven verdict with no AC line to name is never read as a reason" {
  printf '#4752 against http://localhost:3340\n#4752 verify in prod: Unproven\n' > "$BATS_TEST_TMPDIR/prove.out"
  run cws_py "print(cws.failed_reason({'name': 'cicd-land', 'dagRunId': 'l1', 'params': 'CARD=4752'}, $(node)))"
  echo "$output"
  [[ "$output" == "Unproven: no AC named in the prover's output" ]] || return 1
}

@test "AC3 negative proof: a met-only output is not dressed up as a reason" {
  prove_log
  grep -v 'unmeasured\|werk-node-modules' "$BATS_TEST_TMPDIR/prove.out" > "$BATS_TEST_TMPDIR/p2" && mv "$BATS_TEST_TMPDIR/p2" "$BATS_TEST_TMPDIR/prove.out"
  run cws_py "print(cws.failed_reason({'name': 'cicd-land', 'dagRunId': 'l1', 'params': 'CARD=4752'}, $(node)))"
  [[ "$output" != *"met (AC"* ]] || return 1
  [[ "$output" == "Unproven: no AC named in the prover's output" ]] || return 1
}

@test "AC2: an AC deferred to a later card is named as deferred, not as a gap" {
  printf '#4772 against x\nac-4772-2    deferred   AC2: isolates targets\n             by #4805: deferred to #4805, which is Later\n#4772 verify in prod: Proven\n' > "$BATS_TEST_TMPDIR/prove.out"
  run cws_py "print(cws.prove_reason('$BATS_TEST_TMPDIR/prove.out'))"
  echo "$output"
  [[ "$output" == *"ac-4772-2 deferred (by #4805: deferred to #4805, which is Later)"* ]] || return 1
}
