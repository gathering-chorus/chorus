#!/usr/bin/env bats
# @test-type: integration
# @domain: spine — the product domain this suite guards (#4334)
# #3927 — the spine-emit conformance check must BLOCK, not merely notify.
#
# The check was correct for months and stopped nothing: it ran only in
# chorus-health, 20 minutes after the land. Twice in one afternoon (#3917's
# test.script.uncovered, #3926's merge.stale_flag) an unregistered emit landed
# and the alarm fired afterward. These tests exist to prove the check can now
# refuse a violation, because a gate nobody has watched fail is not a gate.
load test_helper

SCRIPT="${CHORUS_ROOT}/platform/scripts/test-werk-emit-conformance.sh"
HOOK="${CHORUS_ROOT}/platform/hooks/pre-commit"

@test "the conformance check passes on the current tree" {
  run env CHORUS_ROOT="$CHORUS_ROOT" bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE: an UNREGISTERED emit makes the check FAIL" {
  # Build a world where a werk source emits an event absent from the schema.
  # The script derives its root from its OWN location, so the fixture must place
  # it at the same relative path a real repo would.
  W="$BATS_TEST_TMPDIR/repo"
  mkdir -p "$W/platform/services/werk-fake/src" "$W/platform/scripts" "$W/designing/schemas"
  cp "${CHORUS_ROOT}/designing/schemas/spine-events.json" "$W/designing/schemas/"
  cp "$SCRIPT" "$W/platform/scripts/"
  cat > "$W/platform/services/werk-fake/src/main.rs" <<'RS'
fn main() { emit_spine("merge.totally_unregistered_3927", &role, &card, &trace, &[]); }
RS
  run env CHORUS_ROOT="$W" bash "$W/platform/scripts/test-werk-emit-conformance.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"merge.totally_unregistered_3927"* ]] || return 1
}

# #4336 — these two cases grepped the hook's text for "exit 1" and for the
# script name. They now RUN the hook in a fixture repo: the gates before this
# one are stubbed to pass, so the only thing that can refuse is the emit gate.
hook_world() {  # hook_world <emit-name> — a git repo whose staged werk source emits <emit-name>
  H="$BATS_TEST_TMPDIR/hookrepo"
  mkdir -p "$H/platform/services/werk-fake/src" "$H/platform/scripts" "$H/platform/hooks" "$H/designing/schemas"
  cp "${CHORUS_ROOT}/designing/schemas/spine-events.json" "$H/designing/schemas/"
  cp "$SCRIPT" "$H/platform/scripts/"
  mkdir -p "$H/platform/services/shared"   # the check also reads the failureClass list
  cp "${CHORUS_ROOT}/platform/services/shared/failure_class.rs" "$H/platform/services/shared/"
  cp "$HOOK" "$H/platform/hooks/pre-commit"
  # #3927 reopened 2026-10-03: stub EVERY gate script the hook calls except the emit
  # check, read from the hook itself. A hand list went stale when #4419 added
  # gate-domain-tag.sh, and the missing stub refused first, so this suite went red
  # without the emit gate ever running.
  for g in $(grep -oE 'platform/scripts/[A-Za-z0-9._-]+' "$HOOK" | sed 's#platform/scripts/##' | sort -u); do
    [ "$g" = "$(basename "$SCRIPT")" ] && continue
    printf '#!/bin/bash\nexit 0\n' > "$H/platform/scripts/$g"; chmod +x "$H/platform/scripts/$g"
  done
  printf 'fn main() { emit_spine("%s", &role, &card, &trace, &[]); }\n' "$1" \
    > "$H/platform/services/werk-fake/src/main.rs"
  git -C "$H" init -q && git -C "$H" add -A
}

@test "NEGATIVE: the pre-commit hook refuses a commit whose werk source emits an unregistered event" {
  hook_world "merge.totally_unregistered_3927"
  run bash -c "cd '$H' && bash platform/hooks/pre-commit"
  [ "$status" -ne 0 ]
  [[ "$output" == *"a werk spine emit is not registered"* ]] || return 1
  [[ "$output" == *"merge.totally_unregistered_3927"* ]] || return 1
  # it stopped HERE: a later gate's refusal would mean this one only logged
  [[ "$output" != *"repo rule"* ]] || return 1
}

@test "control: the same hook with a registered emit does not refuse at the emit gate" {
  ev="$(jq -r '(.events // .) | if type=="object" then keys[0] else .[0].name end' "${CHORUS_ROOT}/designing/schemas/spine-events.json")"
  [ -n "$ev" ] && [ "$ev" != "null" ]
  hook_world "$ev"
  run bash -c "cd '$H' && bash platform/hooks/pre-commit"
  [[ "$output" != *"a werk spine emit is not registered"* ]] || return 1
}
