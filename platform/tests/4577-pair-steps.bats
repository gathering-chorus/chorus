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
