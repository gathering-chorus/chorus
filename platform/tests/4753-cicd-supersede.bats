#!/usr/bin/env bats
# @test-type: unit — fixture runs and stub dagu/chorus-log; no live dagu, no spine
# @domain: cicd
# @card: 4753 · owner: kade
# What Jeff sees: one go per card. When a card's new cicd run starts, its older
# run waiting at his go is stopped, by name, with a card.run.superseded event —
# never answered as Jeff (Jeff 10-26 12:11: "ask for go with dont go in fine print").

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SUP="$REPO/platform/scripts/cicd-supersede"

sup_py() {
  python3 - "$SUP" "$1" <<'EOF'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("sup", sys.argv[1])
spec = importlib.util.spec_from_loader("sup", loader)
sup = importlib.util.module_from_spec(spec)
loader.exec_module(sup)
exec(sys.argv[2])
EOF
}

RUNS="[{'name': 'cicd', 'dagRunId': 'old', 'statusLabel': 'waiting', 'params': 'CARD=4749 ROLE=abby-normal'},
       {'name': 'cicd', 'dagRunId': 'done', 'statusLabel': 'succeeded', 'params': 'CARD=4749 ROLE=abby-normal'},
       {'name': 'cicd', 'dagRunId': 'live', 'statusLabel': 'running', 'params': 'CARD=4749 ROLE=abby-normal'},
       {'name': 'cicd-land', 'dagRunId': 'land', 'statusLabel': 'waiting', 'params': 'CARD=4749 ROLE=abby-normal'}]"

@test "a card's cicd run waiting at the go is superseded; finished, running and land runs are not" {
  run sup_py "print(sup.superseded($RUNS))"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "['old']" ]
}

@test "the older run is stopped and the stop is on the spine" {
  bin="$BATS_TEST_TMPDIR/bin"; mkdir -p "$bin"
  printf '#!/bin/sh\necho "$@" >> "%s/dagu.calls"\n' "$BATS_TEST_TMPDIR" > "$bin/dagu"
  printf '#!/bin/sh\necho "$@" >> "%s/log.calls"\n' "$BATS_TEST_TMPDIR" > "$bin/chorus-log"
  chmod +x "$bin/dagu" "$bin/chorus-log"
  export DAGU="$bin/dagu" CHORUS_LOG_BIN="$bin/chorus-log"
  run sup_py "sup.supersede('4749', 'abby-normal', $RUNS)"
  echo "$output"
  [ "$status" -eq 0 ]
  grep -q "stop cicd --run-id old" "$BATS_TEST_TMPDIR/dagu.calls"
  [ "$(wc -l < "$BATS_TEST_TMPDIR/dagu.calls")" -eq 1 ]
  grep -q "card.run.superseded abby-normal card=4749 run_id=old" "$BATS_TEST_TMPDIR/log.calls"
}

# NEGATIVE PROOF: the other card's waiting run, and a near-miss card number,
# are never touched (CARD=4749 must not match CARD=47490).
@test "negative proof: only this card's runs are superseded" {
  run sup_py "print(sup.superseded([{'name': 'cicd', 'dagRunId': 'x', 'statusLabel': 'waiting', 'params': 'CARD=47490 ROLE=kade'}], card='4749'))"
  echo "$output"
  [ "$output" = "[]" ]
}

# NEGATIVE PROOF: dagu unreadable stops nothing and says so, never "none superseded".
@test "negative proof: unreadable runs supersede nothing and say why" {
  run sup_py "sup.supersede('4749', 'kade', None)"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"dagu unreadable"* ]] || return 1
}

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

# AC2 — /cws shows a stopped older run as superseded, never as failed or waiting.
@test "/cws reads an aborted older cicd run as superseded" {
  run cws_py "
runs = [{'name': 'cicd', 'dagRunId': 'new', 'statusLabel': 'waiting', 'nodes': [{'step': {'name': 'skill-werk-go'}, 'statusLabel': 'waiting'}]},
        {'name': 'cicd', 'dagRunId': 'old', 'statusLabel': 'aborted', 'nodes': [{'step': {'name': 'skill-werk-go'}, 'statusLabel': 'aborted'}]}]
print(cws.dagu_state(runs, 1)[0], cws.dagu_state(runs, 0)[0])"
  echo "$output"
  [ "$output" = "superseded waiting" ] || return 1
}

# NEGATIVE PROOF: the newest run aborted (nothing newer) is not called superseded.
@test "negative proof: an aborted newest run is not superseded" {
  run cws_py "
runs = [{'name': 'cicd', 'dagRunId': 'only', 'statusLabel': 'aborted', 'nodes': [{'step': {'name': 'skill-werk-build'}, 'statusLabel': 'aborted'}]}]
print(cws.dagu_state(runs, 0)[0])"
  echo "$output"
  [ "$output" != "superseded" ] || return 1
}
