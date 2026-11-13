#!/usr/bin/env bats
# @test-type: integration — reads two run folders werk-test wrote; runs nothing
# @domain: tests
# @card: #4965 · owner: kade
# AC3: the same --scope=all run of #4965's werk shows no suite red (fail or unmeasured)
# that passed in the 2026-11-13 nightly. Same run as AC1, same root/head refusal.

load lib/card-run-4965.bash
WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

fixture() {  # fixture <dir> <kind|path|owner|verdict ...>
  local d="$1" l; shift; mkdir -p "$d"; : > "$d/suites-queue"; : > "$d/suites-perf"
  for l in "$@"; do printf '1\tSUITE|%s|summary\n' "$l" >> "$d/suites-queue"; done
}

@test "AC3: nothing green in the nightly is red in this werk's all-scope run" {
  run card_run "$WERK"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  d="$output"
  new="$(newly_red "$NIGHTLY_4965" "$d")"
  [ -z "$new" ] || { echo "newly red in $d:"; echo "$new"; return 1; }
}

@test "AC3 NEGATIVE PROOF: a suite green in the nightly and red or unmeasured now is named" {
  n="$BATS_TEST_TMPDIR/n"; c="$BATS_TEST_TMPDIR/c"
  fixture "$n" "bats|a.bats|kade|pass" "npm|pkg|wren|pass" "bats|was-red.bats|kade|fail"
  fixture "$c" "bats|a.bats|kade|fail" "npm|pkg|wren|unmeasured" "bats|was-red.bats|kade|fail"
  [ "$(newly_red "$n" "$c" | sort | paste -sd';' -)" = "bats|a.bats fail;npm|pkg unmeasured" ]
  fixture "$c" "bats|a.bats|kade|pass" "npm|pkg|wren|pass"
  [ -z "$(newly_red "$n" "$c")" ]
}
