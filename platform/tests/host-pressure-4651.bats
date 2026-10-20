#!/usr/bin/env bats
# @test-type: unit
# @domain: alerts — placed by chorus-crawl's domain rules (#4538)
#
# #4651 — host-pressure fires on the kernel's own memory-pressure level reaching
# critical, not on swap %. On 2026-10-19 it fired 9 times with swap at 91-95% and
# the kernel at level 2 (warn) each time it was sampled; the services all
# answered. Swap alone cannot separate "busy" from "cannot serve".

setup() {
  # Named by its folder so werk-test sees this suite exercise the check (#4419).
  CHECK="${BATS_TEST_DIRNAME}/../../proving/domains/alerts/host-pressure.check.sh"
}

reading() { HOST_PRESSURE_LEVEL_NOW="$1" HOST_PRESSURE_FREE_NOW="$2" HOST_PRESSURE_SWAP_NOW="$3" bash "$CHECK"; }

@test "NEGATIVE PROOF: swap 94% at level 2 (warn) stays quiet" {
  run reading 2 45 94
  [ "$status" -eq 0 ]
  [[ "$output" == ok:* ]] || return 1
}

@test "NEGATIVE PROOF: swap 94% at level 1 (normal) stays quiet" {
  run reading 1 45 94
  [ "$status" -eq 0 ]
}

@test "replay: the three sampled 10-19 alerts (level 2, swap 91-95%) fire 0 times" {
  # card comments: 21:25 swap 95 free 36, 22:20 swap 91 free 38, 23:25 swap 92 free 45
  for row in "2 36 95" "2 38 91" "2 45 92"; do
    run reading $row
    [ "$status" -eq 0 ]
  done
}

@test "level 4 (critical) fires even at swap 40%, naming the condition" {
  run reading 4 30 40
  [ "$status" -eq 1 ]
  [[ "$output" == host-under-pressure:* ]] || return 1
}

@test "the observation carries level, threshold, free and swap" {
  run reading 2 45 94
  [[ "$output" == *"level=2 (warn; fires at 4 critical)"* ]] || return 1
  [[ "$output" == *"free_pct=45"* ]] || return 1
  [[ "$output" == *"swap_used_pct=94"* ]] || return 1
}

@test "the critical alert names the largest process" {
  run reading 4 30 40
  [[ "$output" == *"largest="*MB* ]] || return 1
}

@test "an unreadable level reads unknown and does not fire" {
  run reading "" 45 94
  [ "$status" -eq 0 ]
  [[ "$output" == host-pressure-unknown:* ]] || return 1
}

@test "a non-numeric level reads unknown and does not fire" {
  run reading abc 45 94
  [ "$status" -eq 0 ]
  [[ "$output" == host-pressure-unknown:* ]] || return 1
}

@test "the live box reads a real level" {
  run bash "$CHECK"
  [[ "$output" == *"level="* ]] || return 1
}

@test "a PATH without /usr/sbin (the werk pipeline's, run 2) still reads a level" {
  run env PATH=/usr/bin:/bin bash "$CHECK"
  [[ "$output" == *"level="* ]] || return 1
}
