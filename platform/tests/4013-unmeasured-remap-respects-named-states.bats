#!/usr/bin/env bats
# @test-type: unit — hermetic: drives the remap on fixture summary strings. No
# suites, no registry, no network.
#
# #4013 — #4004 and #4009 landed hours apart on 2026-08-26 and now fight in
# nightly-suites.sh. #4004 makes a suite that DECLINES to run say so:
#
#   0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)
#
# #4009's remap matches "0 pass, 0 fail"* and overwrites it with
#
#   0 pass, 0 fail (UNMEASURED — suite produced no parseable output)
#
# Both cards exist to stop one row meaning two things, and landing them
# separately reintroduced exactly the merge each removed: "working as designed"
# and "produced nothing readable" become one row again, with different owners.
#
# The skip alone is not provable — a guard that never rewrites anything passes
# it trivially (Silas's catch). So the control below is load-bearing: a genuinely
# unparseable row must STILL become UNMEASURED.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

# Drive the REAL function out of the script — not a copy. A copy would pass even
# if the script were never fixed, which is the same hollow shape this card is
# about (#3734).
remap() {
  bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _remap_unmeasured pass \"\$1\"" _ "$1"
}

@test "negative proof: a row that already names its state is left alone" {
  # The live 2026-08-26 collision: #4004's self-refusal must survive intact.
  run remap "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)"
  [[ "$output" == *"SELF-REFUSED rc=3"* ]]
  [[ "$output" != *"UNMEASURED"* ]]
}

@test "control: a bare unparseable row STILL becomes UNMEASURED" {
  # Without this the guard could pass by never rewriting anything — which would
  # silently undo #4009 while looking like a fix.
  run remap "0 pass, 0 fail"
  [[ "$output" == "unmeasured|0 pass, 0 fail (UNMEASURED"* ]]
}

@test "control: a measured row is untouched by either branch" {
  run remap "7 pass, 0 fail"
  [[ "$output" == "pass|7 pass, 0 fail" ]]
}

# NOTE (#4013, found writing these): in bats 1.13 an assertion that is FOLLOWED
# by another `run` is not enforced — `run` resets the accumulated status, so the
# earlier failure is swallowed and the test still reports ok. Both halves below
# therefore live in their own test. Never put a second `run` after an assertion
# you rely on.
@test "the verdict, not just the text, is preserved for a self-named row" {
  run remap "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)"
  [[ "$output" == "pass|"* ]]
}

@test "the verdict, not just the text, changes for a bare unparseable row" {
  # A relabel that leaves the verdict wrong is still a lie, so assert both halves.
  run remap "0 pass, 0 fail"
  [[ "$output" == "unmeasured|"* ]]
}

@test "the row emitted by the real loop uses this function, not an inline copy" {
  # If someone re-inlines the case block, the function stops being the thing
  # under test and these proofs would go quietly hollow.
  run bash -c "grep -c '_remap_unmeasured' '$NIGHTLY'"
  [ "$output" -ge 2 ]
}
