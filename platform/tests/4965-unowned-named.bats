#!/usr/bin/env bats
# @test-type: integration — reads two run folders werk-test wrote; runs nothing
# @domain: tests
# @card: #4965 · owner: kade
# AC4: each suite the 2026-11-13 nightly reported as unowned (platform/api ×3,
# platform/pulse ×2) names an owner in #4965's all-scope run of its werk.

load "$BATS_TEST_DIRNAME/../../platform/tests/lib/card-run-4965.bash"
WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

fixture() {  # fixture <dir> <kind|path|owner|verdict ...>
  local d="$1" l; shift; mkdir -p "$d"; : > "$d/suites-queue"; : > "$d/suites-perf"
  for l in "$@"; do printf '1\tSUITE|%s|summary\n' "$l" >> "$d/suites-queue"; done
}

@test "AC4: the nightly's unowned suites name an owner in this werk's all-scope run" {
  [ "$(red_unowned "$NIGHTLY_4965" | wc -l | tr -d ' ')" -eq 5 ] || { echo "the nightly no longer lists five red unowned suites: $(red_unowned "$NIGHTLY_4965" | paste -sd, -)"; return 1; }
  run ac_run "$WERK"   # the same run AC1 reads (Wren 13:07); after the land, the nightly's (20:20)
  [ "$status" -eq 3 ] && skip "$output"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  left="$(still_unowned "$NIGHTLY_4965" "$output")"
  [ -z "$left" ] || { echo "still unowned:"; echo "$left"; return 1; }
}

@test "AC4 NEGATIVE PROOF: a suite still unowned, or missing from the run, is named" {
  n="$BATS_TEST_TMPDIR/n"; c="$BATS_TEST_TMPDIR/c"
  fixture "$n" "npm|platform/pulse|unowned|fail" "coverage|platform/api|unowned|fail" "bats|x.bats|kade|pass" "cargo|green-unowned|unowned|pass"
  fixture "$c" "npm|platform/pulse|unowned|pass"
  [ "$(still_unowned "$n" "$c" | sort | paste -sd';' -)" = "coverage|platform/api absent;npm|platform/pulse unowned" ]
  fixture "$c" "npm|platform/pulse|wren|pass" "coverage|platform/api|kade|pass"
  [ -z "$(still_unowned "$n" "$c")" ]
}
