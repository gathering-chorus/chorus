#!/usr/bin/env bats
# @test-type: integration — runs two bats suites of this werk and reads its git history
# @domain: tests
# @card: #4965 · owner: kade
# AC3 (narrowed by Jeff 2026-11-13 13:01): no suite is red because of this card's own
# code. newly_red (the original AC3 over a whole run) is kept and proven below; proof
# run 034wwPUt's 19 newly red are named on the card, not fixed here.

load "$BATS_TEST_DIRNAME/../../platform/tests/lib/card-run-4965.bash"
WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

fixture() {  # fixture <dir> <kind|path|owner|verdict ...>
  local d="$1" l; shift; mkdir -p "$d"; : > "$d/suites-queue"; : > "$d/suites-perf"
  for l in "$@"; do printf '1\tSUITE|%s|summary\n' "$l" >> "$d/suites-queue"; done
}

# AC3 as narrowed by Jeff 13:01: the three reds this card's own code caused in proof run
# 034wwPUt pass, and the three stretched waits are back at their old budgets.
STRETCH_4965=fec2ec7af   # the commit that stretched them
# Jeff 13:24: headless-connect-4886 and photos-4635 stay at 15 s, named on the card
# (the test-quality gate refuses any edit to them); only jeff-input-delivery goes back
WAIT_FILES_4965='directing/clearing/tests/jeff-input-delivery.test.ts'

# waits_stretched <repo> <rev> — each wait file that differs at <rev> from before the stretch
waits_stretched() {
  local f
  printf '%s\n' "$WAIT_FILES_4965" | while read -r f; do
    git -C "$1" diff --quiet "$STRETCH_4965^" "$2" -- "$f" || echo "$f"
  done
}

# bats_red <bats args> — the bats output when it does not pass; empty when it does
bats_red() { local out; out="$(bats "$@" 2>&1)" && return 0; echo "$out" | grep -E '^not ok|^#' | head -20; [ -n "$out" ] || echo "bats $* failed"; }

@test "AC3 (narrowed, Jeff 13:01): the card's three code reds pass — 4915-quartet-pattern, and 4145 AC7 under an outer run's TRACE" {
  out="$(bats_red "$WERK/platform/tests/4915-quartet-pattern.bats")"
  [ -z "$out" ] || { echo "$out"; return 1; }
  out="$(TRACE=kade4965all1794583436 bats_red -f 'AC7' "$WERK/platform/tests/4145-runner-owns-the-run.bats")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC3 (narrowed, Jeff 13:01, 13:24): the jeff-input-delivery wait is back at its old budget" {
  out="$(waits_stretched "$WERK" HEAD)"
  [ -z "$out" ] || { echo "still stretched since $STRETCH_4965:"; echo "$out"; return 1; }
}

@test "AC3 NEGATIVE PROOF: the stretch commit itself reads jeff-input stretched; a failing bats is named" {
  [ "$(waits_stretched "$WERK" "$STRETCH_4965")" = directing/clearing/tests/jeff-input-delivery.test.ts ]
  printf '@test "planted" { false; }\n' > "$BATS_TEST_TMPDIR/red.bats"
  [[ "$(bats_red "$BATS_TEST_TMPDIR/red.bats")" == *"not ok 1 planted"* ]] || return 1
  printf '@test "planted" { true; }\n' > "$BATS_TEST_TMPDIR/green.bats"
  [ -z "$(bats_red "$BATS_TEST_TMPDIR/green.bats")" ]
}

@test "AC3 NEGATIVE PROOF: a suite green in the nightly and red or unmeasured now is named" {
  n="$BATS_TEST_TMPDIR/n"; c="$BATS_TEST_TMPDIR/c"
  fixture "$n" "bats|a.bats|kade|pass" "npm|pkg|wren|pass" "bats|was-red.bats|kade|fail"
  fixture "$c" "bats|a.bats|kade|fail" "npm|pkg|wren|unmeasured" "bats|was-red.bats|kade|fail"
  [ "$(newly_red "$n" "$c" | sort | paste -sd';' -)" = "bats|a.bats fail;npm|pkg unmeasured" ]
  fixture "$c" "bats|a.bats|kade|pass" "npm|pkg|wren|pass"
  [ -z "$(newly_red "$n" "$c")" ]
}
