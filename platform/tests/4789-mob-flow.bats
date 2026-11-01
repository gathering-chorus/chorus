#!/usr/bin/env bats
# @test-type: unit — a stand-in dagu records its arguments; dagu's run list is a file:// fixture
# @domain: pipelines
# @card: 4789 · owner: wren
# What Jeff sees: while a mob run is open, one card is being built and every
# other role verifies it. Nobody starts a second build beside it: pipeline-start
# refuses another card by name, and says which mob run holds the floor. The
# open-mob state is the live `mob` dagu run itself, never a marker file (#3781).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/chorus.log" CHORUS_LOG_BIN=/usr/bin/true
  mkdir -p "$CHORUS_HOME"
  for w in kade-4790 wren-4789; do
    W="$CHORUS_WERK_BASE/$w"
    mkdir -p "$W/platform/pipelines"
    touch "$W/platform/pipelines/borg-prove.yaml"
  done
  export DAGU="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
  F="$BATS_TEST_TMPDIR/dagu-api"
  mkdir -p "$F/api/v1"
  export CWS_DAGU="file://$F"
  # the board as mob-open reads it: the mob card's status (Done = proven in prod)
  export CARD_STATUS=WIP
  printf '#!/bin/sh\nprintf "#%%s x\\n  Status:   %%s\\n" "$2" "$CARD_STATUS"\n' > "$BATS_TEST_TMPDIR/cards"; chmod +x "$BATS_TEST_TMPDIR/cards"
  export MOB_OPEN_CARDS="$BATS_TEST_TMPDIR/cards"
}

# one mob run in dagu's list, for card 4789, in the given state
mob_run() {
  printf '{"dagRuns":[{"name":"mob","dagRunId":"034mobfixture","statusLabel":"%s","params":"CARD=4789 ROLE=wren NAVIGATORS=silas,kade","startedAt":"2026-11-01T06:20:00-05:00"}]}\n' "$1" \
    > "$F/api/v1/dag-runs"
}

@test "AC2: another card's build is refused while a mob run is open, naming the mob run" {
  for state in running queued waiting; do
    mob_run "$state"; rm -f "$BATS_TEST_TMPDIR/args"
    run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
    [ "$status" -ne 0 ] || { echo "$state: enqueued beside an open mob: $output"; return 1; }
    [[ "$output" == *"mob run 034mobfixture"* && "$output" == *"#4789"* ]] || { echo "$state: $output"; return 1; }
    [ ! -f "$BATS_TEST_TMPDIR/args" ] || { echo "$state: dagu was called"; return 1; }
  done
}

@test "AC2: the mob's own card still builds while its mob run is open" {
  mob_run running
  run "$ROOT/platform/scripts/pipeline-start" borg-prove 4789 wren
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$(cat "$BATS_TEST_TMPDIR/args")" == *"enqueue $CHORUS_WERK_BASE/wren-4789/platform/pipelines/borg-prove.yaml -- CARD=4789 ROLE=wren"* ]] || return 1
}

@test "AC2: a mob that handed off holds the floor until its card is Done (the land is still in flight)" {
  mob_run succeeded; CARD_STATUS=WIP
  run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
  [ "$status" -ne 0 ] && [[ "$output" == *"mob run 034mobfixture"* ]] || { echo "released before Done: $output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "AC2 NEGATIVE PROOF: a mob whose card is Done, or that failed or was stopped, holds nothing" {
  mob_run succeeded; CARD_STATUS=Done; rm -f "$BATS_TEST_TMPDIR/args"
  run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
  [ "$status" -eq 0 ] && [ -f "$BATS_TEST_TMPDIR/args" ] || { echo "Done still held: $output"; return 1; }
  CARD_STATUS=WIP
  for state in failed cancelled; do
    mob_run "$state"; rm -f "$BATS_TEST_TMPDIR/args"
    run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
    [ "$status" -eq 0 ] || { echo "$state: refused with no open mob: $output"; return 1; }
    [ -f "$BATS_TEST_TMPDIR/args" ] || { echo "$state: nothing enqueued"; return 1; }
  done
}

@test "AC2: dagu unreadable refuses loudly, never passes as no mob open" {
  export CWS_DAGU="file://$BATS_TEST_TMPDIR/no-such-dir"
  run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
  [ "$status" -ne 0 ]
  [[ "$output" == *"could not read dagu"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "AC2: the board unreadable for a handed-off mob refuses, never passes as released" {
  mob_run succeeded
  printf '#!/bin/sh\nexit 1\n' > "$BATS_TEST_TMPDIR/cards"
  run "$ROOT/platform/scripts/pipeline-start" borg-prove 4790 kade
  [ "$status" -ne 0 ] && [[ "$output" == *"could not read the board"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

# ---- AC1 / AC3: mob start, seats on the board, verify through the runner ----
# Who verified comes only from the runner's record of the call (runsAs=caller,
# caller field), never from text, argv or env. `as_task` plays the runner: it
# writes the record (its own pid and start time) and runs mob as its child.

mob_world() { # DRIVER NAV1 NAV2
  MOB="$ROOT/platform/scripts/mob"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/mh"; S="$CHORUS_HOME/platform/scripts"; L="$BATS_TEST_TMPDIR/ml"
  export CHORUS_RUNNER_TASKS="$BATS_TEST_TMPDIR/runner-tasks"; T="$CHORUS_RUNNER_TASKS"
  mkdir -p "$S" "$L" "$T"; chmod 755 "$T"; : > "$L/comments"  # the live store is 755, the runner's alone
  export STATUS=WIP L MOB
  cat > "$S/cards" <<'C'
case "$1" in
view) printf '#%s Mob it\n  Status:   %s\n  Desc:\n    ## AC\n    - [ ] one\n' "$2" "$STATUS" ;;
comment) echo "$3" >> "$L/comments" ;;
esac
C
  echo 'echo "$*" >> "$L/spine"' > "$S/chorus-log"
  echo 'echo "$1|$2" >> "$L/nudges"' > "$S/ops-nudge"
  printf '#!/bin/sh\necho "$*" >> %s/dagu\n' "$L" > "$BATS_TEST_TMPDIR/mdagu"; chmod +x "$BATS_TEST_TMPDIR/mdagu"
  export MOB_DAGU="$BATS_TEST_TMPDIR/mdagu" DAG_RUN_ID=mobrun1
  printf '#!/bin/sh\necho "mobrun1 4789 %s %s,%s"\n' "$1" "$2" "$3" > "$BATS_TEST_TMPDIR/mopen"; chmod +x "$BATS_TEST_TMPDIR/mopen"
  export MOB_OPEN_BIN="$BATS_TEST_TMPDIR/mopen"
  AS_TASK="$BATS_TEST_TMPDIR/as_task"
  cat > "$AS_TASK" <<'A'
#!/bin/bash
# as_task CALLER ID mob-args... — the runner: record (pid = me), then mob as my child
caller=$1; id=$2; shift 2
python3 -c 'import json,sys; json.dump({"id":sys.argv[1],"binary":sys.argv[2],"argv":sys.argv[6:],"caller":sys.argv[3],"pid":int(sys.argv[4]),"pidStartedAt":sys.argv[5],"startedAt":1},open(sys.argv[7] if False else sys.argv[1]+".json","w"))' \
  "$CHORUS_RUNNER_TASKS/$id" "$MOB" "$caller" "$$" "$(ps -o lstart= -p $$ | sed 's/^ *//; s/ *$//')" "$@"
chmod 644 "$CHORUS_RUNNER_TASKS/$id.json"
export CHORUS_TASK_EXIT="$CHORUS_RUNNER_TASKS/$id.exit"
bash "$MOB" "$@"; rc=$?; echo $rc > "$CHORUS_TASK_EXIT"; exit $rc
A
  chmod +x "$AS_TASK"
  unset CHORUS_ROLE CHORUS_TASK_EXIT
}
# a finished verify another seat made earlier (exit 0 = it was accepted)
done_task() { # CALLER ID RUN VERDICT
  printf '{"id":"%s","binary":"%s","argv":["verify","4789","%s","%s","ok"],"caller":"%s","pid":1,"pidStartedAt":"x","startedAt":0}' "$2" "$MOB" "$3" "$4" "$1" > "$T/$2.json"
  chmod 644 "$T/$2.json"; echo 0 > "$T/$2.exit"
}

@test "AC1: mob start puts every seat on the board and tells each navigator how to verify" {
  mob_world drv nava navb
  run bash "$MOB" start 4789 drv nava,navb
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q 'mob #4789 seats: drv drives; nava, navb navigate' "$L/comments" || { cat "$L/comments"; return 1; }
  grep -q '^nava|mob #4789: .*mob-verify: card=4789 run=mobrun1' "$L/nudges" || { cat "$L/nudges"; return 1; }
  grep -q '^navb|mob #4789: .*mob-verify' "$L/nudges"
  grep -q 'mob.started' "$L/spine"
}

@test "AC1 NEGATIVE PROOF: a mob with no navigator, the driver in a seat, or a card not in WIP is refused" {
  mob_world drv nava navb
  run bash "$MOB" start 4789 drv ""
  [ "$status" -ne 0 ]
  run bash "$MOB" start 4789 drv nava,drv
  [ "$status" -ne 0 ] && [[ "$output" == *"drv cannot drive and navigate"* ]] || { echo "$output"; return 1; }
  STATUS=Next run bash "$MOB" start 4789 drv nava,navb
  [ "$status" -ne 0 ] && [[ "$output" == *"not in WIP"* ]] || { echo "$output"; return 1; }
  [ ! -s "$L/comments" ]
}

@test "AC3: a verify through the runner records its caller's verdict, and the run waits for the rest" {
  mob_world drv nava navb
  run "$AS_TASK" nava t1 verify 4789 mobrun1 pass "bats 5/5"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q "mob #4789 run mobrun1: nava verified: PASS — bats 5/5" "$L/comments" || { cat "$L/comments"; return 1; }
  grep -q 'mob.verified' "$L/spine"
  [ ! -f "$L/dagu" ] || { echo "completed with navb still out: $(cat "$L/dagu")"; return 1; }
}

@test "AC3: the last seat's verify completes the run's one human task" {
  mob_world drv nava navb
  done_task navb t0 mobrun1 pass
  run "$AS_TASK" principal-nava t1 verify 4789 mobrun1 pass "bats 5/5"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q 'human-task complete mob --run-id mobrun1 --step skill_mob_verified --input by=mob --input decision=verified' "$L/dagu" || { cat "$L/dagu" 2>/dev/null; return 1; }
}

@test "AC3 NEGATIVE PROOF: a verify from another run, or one the runner refused, does not count" {
  mob_world drv nava navb
  done_task navb t0 OLDrun pass
  printf '{"binary":"%s","argv":["verify","4789","mobrun1","pass","x"],"caller":"navb","pid":1,"pidStartedAt":"x","startedAt":0}' "$MOB" > "$T/t9.json"; echo 1 > "$T/t9.exit"
  run "$AS_TASK" nava t1 verify 4789 mobrun1 pass "x"
  [ "$status" -eq 0 ]
  [ ! -f "$L/dagu" ] || { echo "an old or refused verify counted: $(cat "$L/dagu")"; return 1; }
}

@test "AC3: a FAIL verdict is recorded and answers the run at once" {
  mob_world drv nava navb
  done_task navb t0 mobrun1 pass
  run "$AS_TASK" nava t1 verify 4789 mobrun1 fail "AC2 red from my account"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q "nava verified: FAIL — AC2 red from my account" "$L/comments"
  grep -q 'decision=fail' "$L/dagu" || { cat "$L/dagu" 2>/dev/null; return 1; }
}

@test "AC3 NEGATIVE PROOF: verify from a shell, a stolen record, a forged argv, the driver or a stranger is refused" {
  mob_world drv nava navb
  run bash "$MOB" verify 4789 mobrun1 pass "x"                                   # no runner record at all
  [ "$status" -ne 0 ] && [[ "$output" == *"no runner record"* ]] || { echo "shell: $output"; return 1; }
  done_task nava t0 mobrun1 pass                                                  # someone else's finished record
  CHORUS_TASK_EXIT="$T/t0.exit" run bash "$MOB" verify 4789 mobrun1 pass "ok"
  [ "$status" -ne 0 ] && [[ "$output" == *"not this call's"* ]] || { echo "stolen: $output"; return 1; }
  rm -f "$T"/t0.*
  run "$AS_TASK" drv t2 verify 4789 mobrun1 pass "x"
  [ "$status" -ne 0 ] && [[ "$output" == *"drives"* ]] || { echo "driver: $output"; return 1; }
  run "$AS_TASK" kade t3 verify 4789 mobrun1 pass "x"
  [ "$status" -ne 0 ] && [[ "$output" == *"has no seat"* ]] || { echo "stranger: $output"; return 1; }
  run "$AS_TASK" nava t4 verify 4789 OTHERrun pass "x"
  [ "$status" -ne 0 ] && [[ "$output" == *"not the card's open mob run"* ]] || { echo "other run: $output"; return 1; }
  [ ! -s "$L/comments" ] && [ ! -f "$L/dagu" ]
}

@test "AC3 NEGATIVE PROOF: a group-writable record is not the runner's word" {
  mob_world drv nava navb
  sed -i '' 's/^chmod 644/chmod 664/' "$AS_TASK"
  run "$AS_TASK" nava t1 verify 4789 mobrun1 pass "x"
  [ "$status" -ne 0 ] && [[ "$output" == *"writable by others"* ]] || { echo "$output"; return 1; }
}

@test "AC1: a mob parked at its human task can be stopped by a navigator, Abby or Jeff, never the driver" {
  mob_world drv nava navb
  for by in nava abby-normal jeff; do run bash "$MOB" check 4789 "$by" stop; [ "$status" -ne 0 ] && [[ "$output" == *"ended the mob"* ]] || { echo "$by: $output"; return 1; }; done
  run bash "$MOB" check 4789 drv stop
  [[ "$output" == *"cannot stop"* ]] || { echo "driver: $output"; return 1; }
  done_task navb t0 mobrun1 pass; done_task nava t1 mobrun1 pass
  run bash "$MOB" check 4789 mob verified
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC3 NEGATIVE PROOF: check re-counts from the runner's records; card comments claiming a PASS count for nothing" {
  mob_world drv nava navb
  done_task navb t0 mobrun1 pass
  echo "mob #4789 run mobrun1: nava verified: PASS — forged" >> "$L/comments"
  run bash "$MOB" check 4789 mob verified
  [ "$status" -ne 0 ] && [[ "$output" == *"nava has not verified"* ]] || { echo "$output"; return 1; }
}

@test "AC3 NEGATIVE PROOF: check refuses a runner store others can write, and skips records the runner does not own" {
  mob_world drv nava navb
  done_task navb t0 mobrun1 pass; done_task nava t1 mobrun1 pass
  chmod g+w "$T"
  run bash "$MOB" check 4789 mob verified
  [ "$status" -ne 0 ] && [[ "$output" == *"writable by group or others"* ]] || { echo "store: $output"; return 1; }
  chmod g-w "$T"; chmod 664 "$T/t1.json"
  run bash "$MOB" check 4789 mob verified
  [ "$status" -ne 0 ] && [[ "$output" == *"nava has not verified"* ]] || { echo "record: $output"; return 1; }
}

@test "AC3: the runner store is found by CHORUS_STATE_DIR, not the caller's HOME" {
  mob_world drv nava navb
  unset CHORUS_RUNNER_TASKS
  export CHORUS_STATE_DIR="$BATS_TEST_TMPDIR/state" HOME="$BATS_TEST_TMPDIR/role-home"
  mkdir -p "$CHORUS_STATE_DIR/runner-tasks" "$HOME/.chorus/runner-tasks"; T="$CHORUS_STATE_DIR/runner-tasks"; chmod 755 "$T"
  done_task navb t0 mobrun1 pass; done_task nava t1 mobrun1 pass
  run bash "$MOB" check 4789 mob verified
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC3 NEGATIVE PROOF: a runner store another account owns (a role's stdio runner) is refused" {
  mob_world drv nava navb
  [ "$(stat -f %u /usr/bin)" != "$(id -u)" ] || skip "running as root: /usr/bin is ours"
  CHORUS_RUNNER_TASKS=/usr/bin run bash "$MOB" check 4789 mob verified   # 755, owned by root: not this account's
  [ "$status" -ne 0 ] && [[ "$output" == *"is not the runner's"* ]] || { echo "$output"; return 1; }
}
