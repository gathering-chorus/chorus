#!/usr/bin/env bats
# @test-type: integration — reads two run folders werk-test wrote; runs nothing
# @domain: tests
# @card: #4965 · owner: kade
# AC1 (narrowed by Jeff 2026-11-13 13:01, Wren 13:07): the 18 nightly reds proof run
# 034wwPUt showed green pass there. The run must have tested this werk (AC7's root) at a
# commit HEAD descends from, and every file changed since must be on the after-proof list.
# After the land (Wren 20:20): on any other tree it reads the newest finished run of that
# tree at a head holding these checks (ac_run); before the first such run it skips, saying so.

load "$BATS_TEST_DIRNAME/../../platform/tests/lib/card-run-4965.bash"
WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

fixture() {  # fixture <dir> <kind|path|owner|verdict ...>
  local d="$1" l; shift; mkdir -p "$d"; : > "$d/suites-queue"; : > "$d/suites-perf"
  for l in "$@"; do printf '1\tSUITE|%s|summary\n' "$l" >> "$d/suites-queue"; done
}

@test "AC1 (narrowed, Jeff 13:01): the 18 proven suites pass in proof run 034wwPUt, and the werk moved past it only by the listed files" {
  run ac_run "$WERK"
  [ "$status" -eq 3 ] && skip "$output"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  bad="$(proven_not_pass "$output")"
  [ -z "$bad" ] || { echo "not a pass in $output:"; echo "$bad"; return 1; }
}

# a tree like canonical after the land: a git repo holding the check file, and a runs dir
landed() {  # landed <dir> — prints "<tree> <runs> <head>"
  local t="$1/tree" r="$1/runs"; mkdir -p "$t/platform/tests/lib" "$r"; git -C "$t" init -q
  echo x > "$t/a"; git -C "$t" add -A; git -C "$t" -c user.email=t@x.invalid -c user.name=t commit -qm before
  local before; before=$(git -C "$t" rev-parse HEAD)
  echo lib > "$t/platform/tests/lib/card-run-4965.bash"; git -C "$t" add -A
  git -C "$t" -c user.email=t@x.invalid -c user.name=t commit -qm land
  echo "$t $r $before $(git -C "$t" rev-parse HEAD)"
}
nightly() {  # nightly <runs> <name> <tree> <head> <one red key or ""> — a finished run of all 18
  local d="$1/$2" rows=() k; while read -r k; do
    if [ "$k" = "$5" ]; then rows+=("$k|kade|fail"); else rows+=("$k|kade|pass"); fi
  done <<< "$PROVEN_4965"
  fixture "$d" "${rows[@]}"; echo finished > "$d/suites-perf"; echo "$3" > "$d/root"; echo "$4" > "$d/head"
}

@test "AC1 after the land: the newest run at a head holding the checks is read; one of the 18 red there fails" {
  read -r t r before head <<< "$(landed "$BATS_TEST_TMPDIR")"
  PROOF_4965="$BATS_TEST_TMPDIR/no-proof-run"; RUNS_4965="$r"
  # only a run from before the land: nothing to read yet — skip, never a pass or a false red
  nightly "$r" n0 "$t" "$before" ""
  run ac_run "$t"; [ "$status" -eq 3 ] && [[ "$output" == *"first nightly after the land"* ]] || { echo "$status $output"; return 1; }
  # NEGATIVE PROOF: the first run after the land has one of the 18 red; it is read and named
  sleep 1; nightly "$r" n1 "$t" "$head" "npm|platform/pulse"
  run ac_run "$t"; [ "$status" -eq 0 ] && [ "$output" = "$r/n1" ] || { echo "$status $output"; return 1; }
  [ "$(proven_not_pass "$output")" = "npm|platform/pulse fail" ] || return 1
  # a run of another tree is never read
  sleep 1; nightly "$r" other /some/other/tree "$head" ""
  run ac_run "$t"; [ "$output" = "$r/n1" ] || { echo "$output"; return 1; }
  # control: the next run, all 18 green, passes
  sleep 1; nightly "$r" n2 "$t" "$head" ""
  run ac_run "$t"; [ -z "$(proven_not_pass "$output")" ] || return 1
}

@test "AC1 NEGATIVE PROOF: a proof run whose head is not an ancestor, or a later change off the list, is refused" {
  r="$BATS_TEST_TMPDIR/repo"; mkdir -p "$r"; git -C "$r" init -q
  g() { git -C "$r" -c user.email=t@x.invalid -c user.name=t "$@"; }
  echo a > "$r/x"; g add -A; g commit -qm base; base=$(g rev-parse HEAD)
  d="$BATS_TEST_TMPDIR/run"; mkdir -p "$d"; echo x > "$d/suites-perf"; echo "$r" > "$d/root"; echo "$base" > "$d/head"
  mkdir -p "$r/platform/tests/lib"; echo f > "$r/platform/tests/lib/quartet-walk.sh"; g add -A; g commit -qm fix
  run proof_run "$r" "$d"; [ "$status" -eq 0 ] || { echo "control: $output"; return 1; }
  echo b > "$r/x"; g commit -qam stray
  run proof_run "$r" "$d"; [ "$status" -eq 1 ] && [[ "$output" == *"not on the list: x"* ]] || { echo "$output"; return 1; }
  echo 1111111111111111111111111111111111111111 > "$d/head"
  run proof_run "$r" "$d"; [ "$status" -eq 1 ] && [[ "$output" == *"not an ancestor"* ]] || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE PROOF: one of the 18 failing or missing in the run is named" {
  c="$BATS_TEST_TMPDIR/c"; rows=()
  while read -r k; do rows+=("$k|kade|pass"); done <<< "$PROVEN_4965"
  fixture "$c" "${rows[@]}"
  [ -z "$(proven_not_pass "$c")" ] || return 1
  rows[0]="bats|platform/tests/4416-no-raised-test-timeouts.bats|kade|fail"; unset 'rows[1]'
  fixture "$c" "${rows[@]}"
  [ "$(proven_not_pass "$c" | paste -sd';' -)" = "bats|platform/tests/4416-no-raised-test-timeouts.bats fail;bats|platform/tests/4773-deploy-posts-manifest.bats absent" ]
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
