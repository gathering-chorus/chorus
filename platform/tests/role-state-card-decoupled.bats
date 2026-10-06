#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# @domain: roles — the product domain this suite guards (#4334)
# role-state-card-decoupled.bats — #2467 wave 2 (AC5)
#
# Asserts that no skill source file passes `card=` or `type=` arguments
# to the role-state CLI. Card belongs to the board; role-state owns
# session/attention metadata only (Jeff 2026-04-30 directive).
#
# The Rust role_state.rs writer (wave 1, PR #72) silently drops these
# args, so the skills don't break — but the literal instruction text
# still lives in skill markdown until this gate goes green and the
# files get cleaned up.
#
# This test is the TDD anchor for AC5: red against current main, green
# after the skill source edits land.
#
# #4336 — the skill / fixture / CLAUDE.md cases no longer grep source text.
# Each now RUNS the thing that would pass card=: the role-state commands the
# skills and the generated CLAUDE.md instruct are executed through the real
# CLI (which refuses card=/type=), and the role-state helper suite is run
# with every call it makes recorded.

# Default to the repo root the test file lives in (works in any worktree
# per the per-role-worktree convention), not a hardcoded /chorus path.
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
SKILLS_DIR="$CHORUS_ROOT/skills"
SHIM_BIN="$CHORUS_ROOT/platform/services/chorus-hooks/target/release/chorus-hook-shim"
RUN_INSTRUCTED="$CHORUS_ROOT/platform/tests/fixtures/4336/run-instructed-role-state.py"

# run_instructed <file|dir>... — execute every instructed role-state command
# through the real CLI in a sandbox (own HOME, own spine, test context).
run_instructed() {
  mkdir -p "$BATS_TEST_TMPDIR/home"
  run env HOME="$BATS_TEST_TMPDIR/home" CHORUS_CONTEXT=test \
    CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log" \
    python3 "$RUN_INSTRUCTED" "$SHIM_BIN" "$@"
  echo "$output"
}

@test "every role-state command a skill instructs runs through the CLI without a card=/type= refusal" {
  [ -x "$SHIM_BIN" ] || skip "UNMEASURED — shim binary not built (#4336)"
  run_instructed "$SKILLS_DIR"
  [ "$status" -eq 0 ]
  [[ "$output" == *"refused=0"* ]] || return 1
  # a skills tree with nothing to run is a vanished target, not a pass
  [[ "$output" != *"ran=0 "* ]] || return 1
}

@test "NEGATIVE: a skill that instructs role-state card= / type= is refused when run" {
  [ -x "$SHIM_BIN" ] || skip "UNMEASURED — shim binary not built (#4336)"
  d="$BATS_TEST_TMPDIR/skills/bad"
  mkdir -p "$d"
  printf '%s\n' '1. Declare: `role-state <you> building card=4336`' \
    '```' 'role-state <role> blocked type=fix detail="x"' '```' \
    '2. Then: `role-state <you> waiting`' > "$d/SKILL.md"
  run_instructed "$BATS_TEST_TMPDIR/skills"
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED "*"SKILL.md:1: role-state silas building card=4336"* ]] || return 1
  [[ "$output" == *"REFUSED "*"SKILL.md:3: role-state silas blocked type=fix"* ]] || return 1
  [[ "$output" == *"ran=1 refused=2"* ]] || return 1
}

# --- #2629 wave 3: affordance-layer assertions ---

@test "role-state CLI refuses card= arg with non-zero exit" {
  shim="$CHORUS_ROOT/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  [ -x "$shim" ] || skip "shim binary not built"
  run "$shim" role-state silas building card=99
  [ "$status" -ne 0 ]
  [[ "$output" == *REFUSED* ]] || [[ "$output" == *"#2467"* ]] || [[ "$output" == *"#2629"* ]]
}

@test "role-state CLI refuses type= arg with non-zero exit" {
  shim="$CHORUS_ROOT/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  [ -x "$shim" ] || skip "shim binary not built"
  run "$shim" role-state silas building type=fix
  [ "$status" -ne 0 ]
}

@test "role-state CLI accepts state-only call without error" {
  shim="$CHORUS_ROOT/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  [ -x "$shim" ] || skip "shim binary not built"
  # Use a synthetic role to avoid mutating live silas/wren/kade state
  # (axis-4: no live-role identifiers in tests).
  run "$shim" role-state synthetic-bats-role building
  # Expected: succeeds OR fails with role-specific error (not card-related)
  [[ "$output" != *"card="* ]] || return 1
  [[ "$output" != *"type="* ]] || return 1
}

# #4336 — was a grep of platform/scripts + chorus-hooks/tests for invocation
# patterns. Now the role-state helper (test-role-state-spine.sh) is RUN against
# the real CLI through a recording role-state: every call it makes is logged
# with its exit code. Live instructions carry no card=/type=; the only card=
# a helper may pass is a refusal proof, and it must actually be refused (2).
@test "the role-state helper suite drives the CLI without card= (every call recorded)" {
  [ -x "$SHIM_BIN" ] || skip "UNMEASURED — shim binary not built (#4336)"
  T="$BATS_TEST_TMPDIR/helper"
  mkdir -p "$T/root/platform/scripts" "$T/bin" "$T/home"
  # the real wrapper, reached under the name role-state (it dispatches on $0)
  mkdir -p "$T/real" && ln -s "$CHORUS_ROOT/platform/scripts/shim-wrapper.sh" "$T/real/role-state"
  printf '#!/bin/bash\n"%s/real/role-state" "$@"; rc=$?\necho "$rc $*" >> "%s/calls"\nexit $rc\n' "$T" "$T" \
    > "$T/root/platform/scripts/role-state"
  # the wrapper's trace-hop POST goes to a recorder, never to chorus-api
  printf '#!/bin/bash\nexit 0\n' > "$T/bin/curl"
  chmod +x "$T/root/platform/scripts/role-state" "$T/bin/curl"
  run env PATH="$T/bin:$(dirname "$SHIM_BIN"):$PATH" HOME="$T/home" CHORUS_CONTEXT=test \
    CHORUS_ROOT="$T/root" CHORUS_LOG_FILE="$T/spine.log" \
    bash "$CHORUS_ROOT/platform/scripts/test-role-state-spine.sh"
  echo "$output"; echo "--- calls:"; cat "$T/calls"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Results: 3 passed, 0 failed"* ]] || return 1
  [ -s "$T/calls" ] || return 1
  # every call carrying card= or type= was refused with exit 2
  bad="$(awk '/ (card|type)=/ && $1 != 2' "$T/calls")"
  [ -z "$bad" ] || { echo "helper passed card=/type= and it was NOT refused: $bad"; return 1; }
  # and the state-only calls went through
  [ -n "$(awk '!/ (card|type)=/ && $1 == 0' "$T/calls")" ] || return 1
}

# #4336 — was a grep of the fragment sources. Now the generator runs on a copy
# of designing/claudemd and the GENERATED CLAUDE.md files (+ TEAM_PROTOCOL.md)
# are checked: every role-state command they instruct runs through the CLI
# without refusal, and no generated line tells a role to declare 'building card='.
@test "generated CLAUDE.md: every instructed role-state command runs without a card= refusal" {
  [ -x "$SHIM_BIN" ] || skip "UNMEASURED — shim binary not built (#4336)"
  G="$BATS_TEST_TMPDIR/gen"
  mkdir -p "$G/designing/claudemd" "$G/roles/wren" "$G/roles/silas" "$G/roles/kade" "$G/roles/abby-normal"  # #4432: every manifest role needs its home
  cp -R "$CHORUS_ROOT/designing/claudemd/." "$G/designing/claudemd/"
  ( cd "$G" && env -u CLAUDEMD_BUMP python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
      "$G/designing/claudemd/manifest.json" "$G/designing/claudemd" generate "" "" ) >/dev/null 2>&1 || true
  for r in wren silas kade; do
    [ -s "$G/roles/$r/CLAUDE.md" ] || { echo "generator produced no CLAUDE.md for $r"; return 1; }
  done
  run_instructed "$G/roles/wren/CLAUDE.md" "$G/roles/silas/CLAUDE.md" "$G/roles/kade/CLAUDE.md" \
    "$BATS_TEST_TMPDIR/TEAM_PROTOCOL.md"
  [ "$status" -eq 0 ]
  [[ "$output" == *"refused=0"* ]] || return 1
  live="$(cat "$G"/roles/*/CLAUDE.md | grep 'building card=' | grep -v -E '#2467|deprecated|removed|historical|retired' || true)"
  [ -z "$live" ] || { echo "generated CLAUDE.md carries live 'building card=': $live"; return 1; }
}
