#!/usr/bin/env bats
# @test-type: unit — hermetic: snapshot tests bring their own ps world; the live
# test spawns and reaps only its own procs under $BATS_TEST_TMPDIR
# #3989 — cards-orphan-reaper: negative proof + selectivity.
#
# The #3734 contract: a reaper that gates box health must be SHOWN catching
# the violation it exists to catch (a real orphaned cards-CLI proc), and
# SHOWN sparing everything it must not touch. Hermetic (#3528): snapshot
# tests bring their own ps world via --ps-file; the live test creates its
# own orphan under $BATS_TEST_TMPDIR.

REAPER="$BATS_TEST_DIRNAME/../scripts/cards-orphan-reaper.sh"

setup() {
  SNAP="$BATS_TEST_TMPDIR/ps.snap"
}

# --- negative proof (snapshot): a matching orphan MUST be flagged ---

@test "flags an orphaned src/cli.ts proc past min-age (violation caught)" {
  cat > "$SNAP" <<'EOF'
99901     1 05:33 node /Users/x/CascadeProjects/chorus/directing/products/cards/src/cli.ts add foo
EOF
  run "$REAPER" --dry-run --ps-file "$SNAP"
  [ "$status" -eq 0 ]
  [[ "$output" == *"would reap pid=99901"* ]]
  [[ "$output" == *"REAPED=1"* ]]
}

@test "flags an orphaned dist/cli.js proc and an npx ts-node proc" {
  cat > "$SNAP" <<'EOF'
99902     1 1-02:00:00 node /Users/x/CascadeProjects/chorus/directing/products/cards/dist/cli.js list
99903     1 45:10 node /Users/x/.npm/_npx/ts-node /Users/x/CascadeProjects/chorus/directing/products/cards/src/cli.ts view 1
EOF
  run "$REAPER" --dry-run --ps-file "$SNAP"
  [[ "$output" == *"would reap pid=99902"* ]]
  [[ "$output" == *"would reap pid=99903"* ]]
  [[ "$output" == *"REAPED=2"* ]]
}

# --- selectivity: everything else MUST be spared ---

@test "spares non-cards orphans, parented cards procs, and young orphans" {
  cat > "$SNAP" <<'EOF'
99904     1 10:00:00 node /Users/x/CascadeProjects/chorus/platform/api/dist/server.js
99905   501 05:33 node /Users/x/CascadeProjects/chorus/directing/products/cards/src/cli.ts add foo
99906     1 00:45 node /Users/x/CascadeProjects/chorus/directing/products/cards/src/cli.ts list
EOF
  run "$REAPER" --dry-run --ps-file "$SNAP"
  [ "$status" -eq 0 ]
  [[ "$output" != *"would reap"* ]]
  [[ "$output" == *"REAPED=0"* ]]
  # the young orphan is skipped as young, not silently invisible
  [[ "$output" == *"skip pid=99906"* ]]
}

# --- the RED-side proof of the proof: a snapshot with no match reaps zero ---

@test "empty snapshot reaps nothing (check can distinguish its two states)" {
  : > "$SNAP"
  run "$REAPER" --dry-run --ps-file "$SNAP"
  [ "$status" -eq 0 ]
  [[ "$output" == *"REAPED=0"* ]]
}

# --- live negative proof: spawn a REAL orphan, reaper kills it ---

@test "live: reaper kills a real ppid=1 cards-CLI orphan and spares a control" {
  # Forge a marker path so the predicate matches without touching the repo.
  mkdir -p "$BATS_TEST_TMPDIR/directing/products/cards/src"
  cat > "$BATS_TEST_TMPDIR/directing/products/cards/src/cli.ts" <<'EOF'
setInterval(function () {}, 1000);
EOF
  # Double-fork: subshell backgrounds node then exits -> node reparents to pid 1.
  ORPHAN_PIDFILE="$BATS_TEST_TMPDIR/orphan.pid"
  # >/dev/null and disown — a background child holding bats' stdout pipe
  # open makes the whole file hang at EOF-wait.
  # 3>&- as well: a background child holding bats' FD 3 open hangs the whole
  # file at EOF-wait even after every test passes.
  ( node "$BATS_TEST_TMPDIR/directing/products/cards/src/cli.ts" >/dev/null 2>&1 3>&- & echo $! > "$ORPHAN_PIDFILE" )
  CONTROL_PIDFILE="$BATS_TEST_TMPDIR/control.pid"
  ( node -e 'setInterval(function(){},1000)' >/dev/null 2>&1 3>&- & echo $! > "$CONTROL_PIDFILE" )
  orphan=$(cat "$ORPHAN_PIDFILE"); control=$(cat "$CONTROL_PIDFILE")
  # wait until both are orphaned to ppid 1
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    o=$(ps -o ppid= -p "$orphan" 2>/dev/null | tr -d ' ')
    [ "$o" = "1" ] && break
    sleep 0.3
  done
  run "$REAPER" --min-age-secs 0
  [[ "$output" == *"reaped pid=$orphan"* ]]
  # orphan gone, control alive
  sleep 0.3
  ! ps -p "$orphan" > /dev/null
  ps -p "$control" > /dev/null
  # cleanup control (it is ours; reaper must not have touched it)
  kill "$control" 2>/dev/null || true
}

teardown() {
  # belt-and-suspenders: never leave our own orphans behind on a red run
  [ -f "$BATS_TEST_TMPDIR/orphan.pid" ] && kill "$(cat "$BATS_TEST_TMPDIR/orphan.pid")" 2>/dev/null
  [ -f "$BATS_TEST_TMPDIR/control.pid" ] && kill "$(cat "$BATS_TEST_TMPDIR/control.pid")" 2>/dev/null
  return 0
}
