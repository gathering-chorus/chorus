#!/usr/bin/env bats
# @test-type: integration — reads two run folders werk-test wrote; runs nothing
# @domain: tests
# @card: #4965 · owner: kade
# AC1: every suite red in the 2026-11-13 nightly, except the five after-land checks,
# passes in one --scope=all run started for #4965 on its werk. The run read must have
# tested this werk at its HEAD (AC7's root/head); any other run is refused.

load lib/card-run-4965.bash
WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

fixture() {  # fixture <dir> <kind|path|owner|verdict ...>
  local d="$1" l; shift; mkdir -p "$d"; : > "$d/suites-queue"; : > "$d/suites-perf"
  for l in "$@"; do printf '1\tSUITE|%s|summary\n' "$l" >> "$d/suites-queue"; done
}

@test "AC1: the nightly's reds, less the five after-land, all pass in this werk's all-scope run" {
  run card_run "$WERK"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  d="$output"
  still="$(reds_still_red "$NIGHTLY_4965" "$d")"
  [ -z "$still" ] || { echo "still not green in $d:"; echo "$still"; return 1; }
}

@test "AC1 NEGATIVE PROOF: one red left (or skipped) in the card run is named" {
  n="$BATS_TEST_TMPDIR/n"; c="$BATS_TEST_TMPDIR/c"
  fixture "$n" "bats|platform/tests/a.bats|kade|fail" "shell|b.sh|wren|unmeasured" "bats|platform/tests/4889-one-cicd-run.bats|kade|fail" "bats|ok.bats|kade|pass"
  fixture "$c" "bats|platform/tests/a.bats|kade|pass" "shell|b.sh|wren|skip" "bats|platform/tests/4889-one-cicd-run.bats|kade|fail" "bats|ok.bats|kade|pass"
  [ "$(reds_still_red "$n" "$c")" = "shell|b.sh skip" ]
  fixture "$c" "bats|platform/tests/a.bats|kade|pass" "shell|b.sh|wren|pass"
  [ -z "$(reds_still_red "$n" "$c")" ]
}

@test "AC1 NEGATIVE PROOF: a run of another tree or an older head is refused, never read" {
  r="$BATS_TEST_TMPDIR/runs"; mkdir -p "$r/kade4965allX"; echo x > "$r/kade4965allX/suites-perf"
  echo "$WERK" > "$r/kade4965allX/root"; echo 0000000 > "$r/kade4965allX/head"
  WERK_TEST_RUNS_4965="$r" RUNS_4965="$r" run card_run "$WERK"
  [ "$status" -eq 1 ] && [[ "$output" == *"the werk is at"* ]] || { echo "$output"; return 1; }
  echo /some/other/tree > "$r/kade4965allX/root"
  RUNS_4965="$r" run card_run "$WERK"
  [ "$status" -eq 1 ] && [[ "$output" == *"no finished all-scope run"* ]] || { echo "$output"; return 1; }
}
