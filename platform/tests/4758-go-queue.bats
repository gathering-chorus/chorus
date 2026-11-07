#!/usr/bin/env bats
# @test-type: integration — a scratch dagu scheduler (its own DAGU_HOME and ports); nothing of prod's dagu is read or written.
# @domain: pipelines
# #4758 — platform/config/dagu and platform/pipelines/cicd-go: a signed go
# starts its land at once, never behind a test run on `werk`. A run waiting at
# a human task frees its queue slot and, on resume, re-queues on its own dag's
# queue: #4747's signed go sat 20 minutes behind two test runs. Measured here
# on dagu 2.18.2 (10-26 18:01): the go half on werk-go landed 6s after the go,
# today's shape on werk 27s (after a 25s test run).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  command -v dagu >/dev/null || skip "dagu is not installed"
  export DAGU_HOME="$BATS_TEST_TMPDIR/home"
  mkdir -p "$DAGU_HOME" "$BATS_TEST_TMPDIR/dags"
  # the queues as prod declares them (the werk and werk-go lines of dagu.yaml)
  werk=$(awk '/- name: werk$/{getline; print $2}' "$ROOT/platform/config/dagu.yaml")
  wgo=$(awk '/- name: werk-go$/{getline; print $2}' "$ROOT/platform/config/dagu.yaml")
  [ "$werk" = 1 ] && [ -n "$wgo" ]
  cat > "$DAGU_HOME/config.yaml" <<EOF
dags_dir: $BATS_TEST_TMPDIR/dags
auth:
  mode: none
queues:
  enabled: true
  config:
    - name: werk
      max_concurrency: $werk
    - name: werk-go
      max_concurrency: $wgo
EOF
  printf 'queue: werk\nsteps:\n  - name: test\n    command: sleep 15\n' > "$BATS_TEST_TMPDIR/dags/slow.yaml"
  # the go half's queue is read from the generated cicd-go, not restated here
  goq=$(awk '/^queue:/{print $2; exit}' "$ROOT/platform/pipelines/cicd-go.yaml")
  for q in "$goq" werk; do
    cat > "$BATS_TEST_TMPDIR/dags/go-$q.yaml" <<EOF
queue: $q
steps:
  - name: go
    id: go
    action: human.task
    with:
      prompt: "go?"
      form:
        type: object
        properties:
          decision:
            type: string
            enum: [go, no]
        required: [decision]
  - name: land
    depends: [go]
    command: "true"
EOF
  done
  PORT=$((18700 + RANDOM % 200)); CPORT=$((50200 + RANDOM % 200))
  # 3>&- : a child holding bats' fd 3 keeps the test open after it ends
  env -u CHORUS_ROLE -u ROLE dagu start-all --port "$PORT" --coordinator.port "$CPORT" > "$BATS_TEST_TMPDIR/sched.log" 2>&1 3>&- &
  SCHED=$!
  sleep 4
}

# The scratch scheduler is this test's own child. On the 10-26 18:21 pipeline
# run it logged "Scheduler stopped" on TERM and never exited, and a bare
# `wait` held the local proof (and the werk queue behind it) open: give it 5s.
teardown() {
  [ -n "${SCHED:-}" ] || return 0
  kill "$SCHED" 2>/dev/null
  for _ in $(seq 1 10); do kill -0 "$SCHED" 2>/dev/null || return 0; sleep 0.5; done
  kill -9 "$SCHED" 2>/dev/null || true
}

D() { env -u CHORUS_ROLE -u ROLE dagu "$@"; }
st() { D status "$1" --run-id "$2" 2>/dev/null | head -1; }
wait_for() { for _ in $(seq 1 "$3"); do [[ "$(st "$1" "$2")" == "$4"* ]] && return 0; sleep 1; done; return 1; }

@test "with a test run holding werk, a go on the go queue lands at once; on werk it waits" {
  [ "$goq" = werk-go ]
  for q in werk-go werk; do D enqueue --run-id "g-$q" "go-$q" >/dev/null 2>&1; done
  wait_for go-werk-go g-werk-go 30 Waiting
  wait_for go-werk g-werk 30 Waiting
  D enqueue --run-id slow-1 slow >/dev/null 2>&1
  wait_for slow slow-1 30 Running
  t0=$(date +%s)
  D human-task complete go-werk-go --run-id g-werk-go --step go --input decision=go >/dev/null
  D human-task complete go-werk --run-id g-werk --step go --input decision=go >/dev/null
  wait_for go-werk-go g-werk-go 12 Succeeded
  fast=$(( $(date +%s) - t0 ))
  # NEGATIVE PROOF: the same go on `werk` is still behind the test run, so the
  # measurement can tell the two queues apart
  [[ "$(st go-werk g-werk)" != Succeeded* ]] || return 1
  wait_for go-werk g-werk 40 Succeeded
  slow=$(( $(date +%s) - t0 ))
  echo "go queue: ${fast}s · werk: ${slow}s" >&3
  [ "$fast" -lt 12 ] && [ "$slow" -gt "$fast" ]
}
