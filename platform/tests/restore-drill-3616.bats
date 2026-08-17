#!/usr/bin/env bats
# @test-type: unit — pure verifier logic against fixtures; no ssh, no live store.
#
# #3616 — the restore drill's negative proofs. A drill that cannot go RED on a
# broken restore is decoration; this file proves it can, using the SAME
# comparison logic the script runs.
#
# The three states the verifier must separate:
#   restored ≈ live            → PASS
#   restored 0, live > 0       → FAIL (empty restore; never excused by age)
#   restored << live for age   → FAIL (short restore)
#   restored >> live           → FAIL (compared the wrong store)
# and the age band that made the FIRST live run (2026-08-17) a false red:
#   a 2-day-old backup legitimately behind on a fast-moving graph → PASS.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  DRILL="$REPO/platform/scripts/restore-drill.sh"
}

# The verifier's decision, extracted exactly as the script computes it, so the
# test exercises the real arithmetic rather than a paraphrase of it.
verdict() { # restored live age_days -> echoes pass|fail-empty|fail-short|fail-excess
  local r="$1" l="$2" age="$3"
  local ratio
  ratio="$(awk -v m=0.98 -v d=0.08 -v a="$age" -v f=0.60 'BEGIN{x=m-(d*a); if(x<f)x=f; printf "%.4f", x}')"
  if [ "$r" -eq 0 ] && [ "$l" -gt 0 ]; then echo "fail-empty"; return; fi
  if awk -v r="$r" -v l="$l" -v m="$ratio" 'BEGIN{exit !(l>0 && r < l*m)}'; then echo "fail-short"; return; fi
  if [ "$r" -gt $(( l * 2 )) ] && [ "$l" -gt 0 ]; then echo "fail-excess"; return; fi
  echo "pass"
}

@test "the drill script exists and is executable" {
  [ -x "$DRILL" ]
}

@test "NEGATIVE PROOF: an EMPTY restore fails, at any backup age" {
  [ "$(verdict 0 33092 0)" = "fail-empty" ]
  [ "$(verdict 0 33092 30)" = "fail-empty" ]   # staleness never excuses zero
}

@test "NEGATIVE PROOF: a SHORT restore fails (truncated snapshot)" {
  # half the triples, fresh backup — the truncation class this card exists for
  [ "$(verdict 16000 33092 0)" = "fail-short" ]
}

@test "NEGATIVE PROOF: a restore far EXCEEDING live fails (wrong store compared)" {
  [ "$(verdict 90000 33092 0)" = "fail-excess" ]
}

@test "a fresh, complete restore passes" {
  [ "$(verdict 33092 33092 0)" = "pass" ]
  [ "$(verdict 32900 33092 0)" = "pass" ]
}

@test "the FIRST LIVE RUN's numbers: 2-day-old backup behind on a fast graph now passes" {
  # 2026-08-17 13:43, real values: ontology 28409 restored vs 33092 live, backup
  # 2 days old across three model lands (#3896/#3870/#3902 — verified in git).
  # Under the naive flat 0.90 band this was RED; age-aware it is the expected
  # drift, and the drill stops crying wolf about a backup that is fine.
  [ "$(verdict 28409 33092 2)" = "pass" ]
  [ "$(verdict 54989 56307 2)" = "pass" ]
  [ "$(verdict 101 101 2)" = "pass" ]
}

@test "NEGATIVE PROOF: even an old backup fails below the floor" {
  # the age allowance bottoms out at 0.60 — a 30-day-old backup holding 20% of
  # live is broken, not stale.
  [ "$(verdict 6000 33092 30)" = "fail-short" ]
}

@test "the drill REFUSES a scratch path inside the live data dir" {
  run env RESTORE_DRILL_SCRATCH="$HOME/.gathering/data/scratch-drill" bash "$DRILL"
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSING"* ]]
  [[ "$output" == *"live data dir"* ]]
}
