#!/usr/bin/env bats
# @test-type: unit — hermetic
# #4131 — "all the skips no outputs all of it" (Jeff 2026-09-09 17:36). Each
# unit that produced no result on the 12:12 run now produces one, and each
# check here ships with the state where it must go RED (#3734).

setup() {
  ROOT="$BATS_TEST_DIRNAME/.."
  TMP="$BATS_TEST_TMPDIR"
}

# --- test-product-membrane.sh: a freshness verdict under the nightly ---------

@test "membrane under the nightly: no attended run recorded is RED, not self-refused" {
  NIGHTLY_UNIT_TIMEOUT=1 MEMBRANE_LEDGER="$TMP/none" run bash "$ROOT/scripts/test-product-membrane.sh"
  [ "$status" -eq 1 ]
  [[ "$output" == *"never been proven"* ]]
  [[ "$output" == *"=== Results: 0 passed, 1 failed ==="* ]]
}

@test "membrane under the nightly: a proof older than the stale bar is RED" {
  echo "$(( $(date +%s) - 40*86400 )) OK" > "$TMP/ledger"
  NIGHTLY_UNIT_TIMEOUT=1 MEMBRANE_LEDGER="$TMP/ledger" run bash "$ROOT/scripts/test-product-membrane.sh"
  [ "$status" -eq 1 ]
  [[ "$output" == *"40d ago"* ]]
}

@test "membrane under the nightly: a fresh attended OK is a PASS with counts" {
  echo "$(( $(date +%s) - 3*86400 )) OK" > "$TMP/ledger"
  NIGHTLY_UNIT_TIMEOUT=1 MEMBRANE_LEDGER="$TMP/ledger" run bash "$ROOT/scripts/test-product-membrane.sh"
  [ "$status" -eq 0 ]
  [[ "$output" == *"=== Results: 1 passed, 0 failed ==="* ]]
}

@test "membrane under the nightly: a FAIL after an OK does not count as the proof" {
  echo "$(( $(date +%s) - 3*86400 )) OK" > "$TMP/ledger"
  echo "$(( $(date +%s) - 1*86400 )) FAIL" >> "$TMP/ledger"
  NIGHTLY_UNIT_TIMEOUT=1 MEMBRANE_LEDGER="$TMP/ledger" run bash "$ROOT/scripts/test-product-membrane.sh"
  # the newest OK is 3d old: still a pass; the ledger line format is exact
  [ "$status" -eq 0 ]
  [[ "$output" == *"3d ago"* ]]
}

@test "membrane attended without the grant still refuses (#4004 kept)" {
  MEMBRANE_LEDGER="$TMP/none" run bash "$ROOT/scripts/test-product-membrane.sh" --dry-run
  [ "$status" -eq 3 ]
}

# --- demo-complete-drift-audit.bats: one indexed pass, open demos excluded ---

_spine_line() { # ts card event
  printf '{"timestamp":"%s","event":"%s","card_id":"%s","role":"kade"}\n' "$1" "$3" "$2"
}
_ago() { date -u -v-"$1"H +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null || date -u -d "$1 hours ago" +"%Y-%m-%dT%H:%M:%S.000Z"; }

@test "demo audit: five old demos with no terminal state is RED" {
  S="$TMP/spine"; : > "$S"
  for c in 1 2 3 4 5; do _spine_line "$(_ago 48)" "$c" "card.demo.started" >> "$S"; done
  CHORUS_SPINE="$S" run bats "$ROOT/tests/demo-complete-drift-audit.bats"
  [ "$status" -ne 0 ]
  [[ "$output" == *"only 0 closed"* ]]
}

@test "demo audit: old demos each closed by an accept is GREEN" {
  S="$TMP/spine"; : > "$S"
  for c in 1 2 3 4 5; do
    _spine_line "$(_ago 48)" "$c" "card.demo.started" >> "$S"
    _spine_line "$(_ago 47)" "$c" "card.accepted" >> "$S"
  done
  CHORUS_SPINE="$S" run bats "$ROOT/tests/demo-complete-drift-audit.bats"
  [ "$status" -eq 0 ]
}

@test "demo audit: demos presented in the last 24h are open, not drift" {
  S="$TMP/spine"; : > "$S"
  for c in 1 2 3 4 5; do _spine_line "$(_ago 2)" "$c" "card.demo.started" >> "$S"; done
  CHORUS_SPINE="$S" run bats "$ROOT/tests/demo-complete-drift-audit.bats"
  [ "$status" -eq 0 ]
}

@test "demo audit: the doubled card.demo.started emit counts one demo" {
  S="$TMP/spine"; : > "$S"
  # 4 real unclosed old demos, each emitted twice = 8 lines; threshold is >3
  # demos, so 4 must still be RED and 2 doubled demos must be GREEN.
  for c in 1 2; do t=$(_ago 48); _spine_line "$t" "$c" "card.demo.started" >> "$S"; _spine_line "$t" "$c" "card.demo.started" >> "$S"; done
  CHORUS_SPINE="$S" run bats "$ROOT/tests/demo-complete-drift-audit.bats"
  [ "$status" -eq 0 ]
  for c in 3 4; do t=$(_ago 48); _spine_line "$t" "$c" "card.demo.started" >> "$S"; _spine_line "$t" "$c" "card.demo.started" >> "$S"; done
  CHORUS_SPINE="$S" run bats "$ROOT/tests/demo-complete-drift-audit.bats"
  [ "$status" -ne 0 ]
  [[ "$output" == *"Found 4 card.demo.started"* ]]
}

# --- spine-emit-drift-audit.bats: indexed lookups keep the RED --------------

@test "spine-emit audit: a done-brief with no card.accepted is RED" {
  R="$TMP/root"; B="$R/directing/products/roles/kade/briefs"; mkdir -p "$B"
  today=$(date -u +"%Y-%m-%d")
  : > "$B/${today}-card-77-done.md"
  S="$TMP/spine"; _spine_line "$(_ago 30)" "99" "card.accepted" > "$S"
  CHORUS_ROOT="$R" CHORUS_SPINE="$S" run bats "$ROOT/tests/spine-emit-drift-audit.bats"
  [ "$status" -ne 0 ]
  [[ "$output" == *"#77"* ]]
}

@test "spine-emit audit: a done-brief whose accept is on the spine is GREEN" {
  R="$TMP/root"; B="$R/directing/products/roles/kade/briefs"; mkdir -p "$B"
  today=$(date -u +"%Y-%m-%d")
  : > "$B/${today}-card-77-done.md"
  S="$TMP/spine"; printf '{"timestamp":"%sT10:00:00.000Z","event":"card.accepted","card_id":"77"}\n' "$today" > "$S"
  CHORUS_ROOT="$R" CHORUS_SPINE="$S" run bats "$ROOT/tests/spine-emit-drift-audit.bats"
  [ "$status" -eq 0 ]
}

# --- nightly-suites.sh: an UNMEASURED row keeps its output on disk -----------

@test "nightly: an unmeasured unit's output is persisted like a red's" {
  export NIGHTLY_FAIL_DIR="$TMP/failures"
  BIN="$TMP/bin"; mkdir -p "$BIN"
  cat > "$BIN/werk-test" <<EOS
echo "SAST: semgrep not installed — SKIPPED"
echo "nightly-unit|security|platform/scripts/x-scan.sh|pass|0 pass, 0 fail"
exit 0
EOS
  chmod +x "$BIN/werk-test"
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$ROOT/scripts/nightly-suites.sh" --run-one security platform/scripts/x-scan.sh
  [[ "$output" == *"|unmeasured|"* ]]
  logp=$(bash -c "source '$ROOT/scripts/nightly-suites.sh'; _fail_log_path security platform/scripts/x-scan.sh")
  [ -f "$logp" ]
  run cat "$logp"
  [[ "$output" == *"semgrep not installed"* ]]
}
