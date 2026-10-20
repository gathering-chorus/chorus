#!/usr/bin/env bats
# @test-type: unit — the pair pipeline's steps against a fake board, spine and dagu; no live service
# @domain: pipelines — /pair as a dagu pipeline (#4577)
# What Jeff sees: a pair starts only on a WIP card, the navigator is told how to
# give the green, only the navigator's green moves the pair on, a red sends the
# pair back to slice with a new run waiting, and the end writes his summary.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PAIR="$ROOT/platform/scripts/pair"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" PAIR_DIR="$BATS_TEST_TMPDIR/pairs"
  S="$CHORUS_HOME/platform/scripts"; L="$BATS_TEST_TMPDIR/log"
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
  printf '#!/usr/bin/env python3\nprint("failed at local-proof · verdict: RED")\n' > "$S/chorus-werk-status"
  printf '#!/usr/bin/env bash\necho "$*" >> %s/dagu\n' "$L" > "$BATS_TEST_TMPDIR/dagu"; chmod +x "$BATS_TEST_TMPDIR/dagu"
  export PAIR_DAGU="$BATS_TEST_TMPDIR/dagu" DAG_RUN_ID=run1
  unset CHORUS_ROLE
}

@test "start registers both roles, writes the AC, and tells the navigator how to give the green" {
  run bash "$PAIR" start 4577 kade silas
  [ "$status" -eq 0 ]
  grep -q '"driver":"kade"' "$CHORUS_HOME/ops/active-pairs/4577.json"
  grep -q '"navigator":"silas"' "$CHORUS_HOME/ops/active-pairs/4577.json"
  grep -q -- '- \[ \] two' "$PAIR_DIR/pair-4577.md"
  grep -q '^silas|pair #4577: you navigate, kade drives.*--run-id run1 --step skill_pair_green --input by=silas --input decision=green' "$L/nudges"
  grep -q '^pair.started system card=4577 driver=kade partner=silas' "$L/spine"
}

@test "negative proof: a card not in WIP is refused and nothing is registered" {
  STATUS=Later run bash "$PAIR" start 4577 kade silas
  [ "$status" -eq 1 ]
  [[ "$output" == *"not in WIP"* ]] || return 1
  [ ! -e "$CHORUS_HOME/ops/active-pairs/4577.json" ]
  [ ! -e "$L/nudges" ]
}

@test "negative proof: one role cannot drive and navigate" {
  run bash "$PAIR" start 4577 kade kade
  [ "$status" -eq 1 ]
}

@test "the navigator's green passes the check" {
  run bash "$PAIR" check 4577 silas silas green
  [ "$status" -eq 0 ]
  grep -q '^pair.green system card=4577' "$L/spine"
}

@test "negative proof: the driver cannot give the navigator's green" {
  run bash "$PAIR" check 4577 silas kade green
  [ "$status" -eq 1 ]
  [[ "$output" == *"must come from the navigator silas"* ]] || return 1
}

@test "a red sends the pair back to slice: a new run is enqueued and both roles hear it" {
  bash "$PAIR" start 4577 kade silas >/dev/null
  run bash "$PAIR" red 4577 kade silas run1
  [ "$status" -eq 0 ]
  grep -q 'enqueue pair -- CARD=4577 ROLE=kade NAVIGATOR=silas' "$L/dagu"
  grep -q '^kade|pair #4577 red: failed at local-proof' "$L/nudges"
  grep -q '^silas|pair #4577 red: failed at local-proof' "$L/nudges"
  # the next run's start is back to slice, not a new pair
  run bash "$PAIR" start 4577 kade silas
  grep -q '^## Back to slice' "$PAIR_DIR/pair-4577.md"
  [ "$(grep -c pair.started "$L/spine")" -eq 1 ]
}

@test "negative proof: a pair that ended is never re-enqueued" {
  run bash "$PAIR" red 4577 kade silas run1
  [ "$status" -eq 0 ]
  [ ! -e "$L/dagu" ]
}

@test "the navigator's stop ends the pair; its red does not loop" {
  bash "$PAIR" start 4577 kade silas >/dev/null
  run bash "$PAIR" check 4577 silas silas stop
  [ "$status" -eq 1 ]
  [ ! -e "$CHORUS_HOME/ops/active-pairs/4577.json" ]
  run bash "$PAIR" red 4577 kade silas run1
  [ ! -e "$L/dagu" ]
}

@test "end writes the For-Jeff summary from the slice log and deregisters" {
  bash "$PAIR" start 4577 kade silas >/dev/null
  bash "$PAIR" slice 4577 "AC two green" >/dev/null
  bash "$PAIR" review 4577 "next" >/dev/null
  run bash "$PAIR" end 4577 kade silas
  [ "$status" -eq 0 ]
  grep -q '^4577|pair #4577 ended: kade drove, silas navigated · 1 slices, 1 reviews · AC 1 of 2 checked' "$L/comments"
  grep -q '^## For Jeff' "$PAIR_DIR/pair-4577.md"
  grep -q 'slice: AC two green' "$PAIR_DIR/pair-4577.md"
  [ ! -e "$CHORUS_HOME/ops/active-pairs/4577.json" ]
  grep -q '^pair.slice system card=4577' "$L/spine"
  grep -q '^pair.ended system card=4577' "$L/spine"
}

# #4577 reopen 1 — Abby's review, 10-17 09:18
@test "Abby or Jeff can end a pair whose navigator is gone" {
  bash "$PAIR" start 4577 kade silas >/dev/null
  run bash "$PAIR" check 4577 silas abby-normal stop
  [[ "$output" == *"abby-normal ended the pair"* ]] || return 1
  [ ! -e "$CHORUS_HOME/ops/active-pairs/4577.json" ]
}

@test "negative proof: Abby cannot give the navigator's green, and the driver cannot stop for him" {
  run bash "$PAIR" check 4577 silas abby-normal green
  [ "$status" -eq 1 ]
  [[ "$output" == *"must come from the navigator silas"* ]] || return 1
  run bash "$PAIR" check 4577 silas kade stop
  [[ "$output" == *"must come from the navigator silas"* ]] || return 1
}

@test "start keeps a pair file that is already there" {
  echo "# notes Wren kept" > "$PAIR_DIR/pair-4577.md"
  run bash "$PAIR" start 4577 kade silas
  [ "$status" -eq 0 ]
  head -1 "$PAIR_DIR/pair-4577.md" | grep -q '^# notes Wren kept'
  grep -q '^# Pair #4577 — kade drives, silas navigates' "$PAIR_DIR/pair-4577.md"
}

# #4663 — `pair end` grepped the pair file into itself: grep read what the same
# block appended, never reached the end, and #4652's end wrote 540 GB to /tmp
# (disk 97%, 2026-10-20 13:20). The end is bounded by a 10 MB file cap and a
# 5 s watchdog here, so the old code fails fast instead of filling the disk.
@test "end with 500 slices finishes within 5 s and appends exactly one copy of them (#4663)" {
  bash "$PAIR" start 4663 kade silas >/dev/null
  pf="$PAIR_DIR/pair-4663.md"
  for i in $(seq 1 500); do echo "- 12:00 slice: piece $i" >> "$pf"; done
  before=$(wc -l < "$pf")
  # a watchdog: the end gets 5 s, then it is killed (a hang reads red, never a full disk)
  # ulimit -f caps any file the end (and its children) writes at 10 MB, so a
  # runaway grep dies on its own instead of outliving the watchdog's kill
  ( ulimit -f 20480; bash "$PAIR" end 4663 kade silas > "$BATS_TEST_TMPDIR/end.out" 2>&1; echo $? > "$BATS_TEST_TMPDIR/end.rc" ) &
  pid=$!
  for _ in $(seq 1 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$pid" 2>/dev/null; then
    pkill -P "$pid" 2>/dev/null; kill "$pid" 2>/dev/null
    echo "pair end still running after 5 s; the file is $(wc -l < "$pf") lines"; return 1
  fi
  [ "$(cat "$BATS_TEST_TMPDIR/end.rc")" -eq 0 ] || { cat "$BATS_TEST_TMPDIR/end.out"; return 1; }
  # the file grew by: a blank line, the heading, the summary, and the 500 slices once
  after=$(wc -l < "$pf")
  [ "$after" -eq $((before + 3 + 500)) ] || { echo "grew by $((after - before)), want 503"; return 1; }
  [ "$(grep -c -- '^- .* slice: ' "$pf")" -eq 1000 ] || { echo "slice lines: $(grep -c -- '^- .* slice: ' "$pf"), want 1000 (500 + one copy)"; return 1; }
  [ "$(grep -c '^## For Jeff' "$pf")" -eq 1 ] || return 1
}
