#!/usr/bin/env bats
# @test-type: unit — a stub borg-prove; nothing real is read or run.
# @domain: cards
# #4760 — platform/scripts/borg-prove-intake: werk-pull's intake at pull hands
# the card to `borg-prove intake`, and its exit is the pull's verdict.

setup() {
  T="$(mktemp -d)"
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/borg-prove-intake"
  printf '#!/bin/sh\necho "$*" > "%s/args"\nexit "${STUB_EXIT:-0}"\n' "$T" > "$T/borg-prove"
  chmod +x "$T/borg-prove"
}

teardown() { mv "$T" "$T.done" 2>/dev/null || true; }

@test "intake hands the card to borg-prove intake" {
  BORG_PROVE="$T/borg-prove" run "$SCRIPT" 4760
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$T/args")" = "intake 4760" ] || { cat "$T/args"; return 1; }
}

# NEGATIVE PROOF: a card borg-prove refuses is refused by the wrapper too
@test "negative proof: a refused intake exits non-zero" {
  BORG_PROVE="$T/borg-prove" STUB_EXIT=1 run "$SCRIPT" 4760
  [ "$status" -ne 0 ] || { echo "status $status"; return 1; }
}
