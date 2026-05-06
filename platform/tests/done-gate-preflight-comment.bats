#!/usr/bin/env bats
# done-gate.sh — accepts demo:preflight-pass card comment as evidence (#2770).
#
# Per /demo skill Step 1.5 (#2090): single-card demos post a card comment
# `demo:preflight-pass ac=N/M — <role>` as evidence. Pre-#2770 done-gate.sh
# only accepted three other forms (brief file, spine event, "Demo started"
# comment). Posting just the prescribed preflight comment refused acp.
#
# These tests stub the cards CLI + chorus-log + curl so they run
# hermetically, no live board / chorus-api required.

setup() {
  TEST_ROOT="$(mktemp -d)"
  export CHORUS_ROOT="$TEST_ROOT/chorus"
  mkdir -p "$CHORUS_ROOT/platform/scripts" "$CHORUS_ROOT/roles/wren/briefs" "$CHORUS_ROOT/skills/demo/gates"

  # Real done-gate.sh under test
  cp "$BATS_TEST_DIRNAME/../../skills/demo/gates/done-gate.sh" "$CHORUS_ROOT/skills/demo/gates/done-gate.sh"
  chmod +x "$CHORUS_ROOT/skills/demo/gates/done-gate.sh"

  # Stub cards CLI — returns a card view template controlled by env vars
  cat > "$CHORUS_ROOT/platform/scripts/cards" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = "view" ]; then
  cat <<EOF
#$2 ${TEST_TITLE:-test card}
  Status:   ${TEST_STATUS:-WIP}
  Owner:    ${TEST_OWNER:-Kade}
  Domains:  ${TEST_DOMAINS:-domain:chorus, type:fix}
  Comments (1):
    [jeff] ${TEST_COMMENT:-no-comment}
EOF
fi
STUB
  chmod +x "$CHORUS_ROOT/platform/scripts/cards"

  # Stub chorus-log — no-op to avoid spine writes
  cat > "$CHORUS_ROOT/platform/scripts/chorus-log" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB
  chmod +x "$CHORUS_ROOT/platform/scripts/chorus-log"
}

teardown() {
  rm -rf "$TEST_ROOT"
}

@test "done-gate accepts demo:preflight-pass comment as evidence (#2770)" {
  TEST_COMMENT="demo:preflight-pass ac=4/4 — kade" \
    run bash "$CHORUS_ROOT/skills/demo/gates/done-gate.sh" 9999 kade
  [ "$status" -eq 0 ]
}

@test "done-gate accepts 'Demo started' comment as evidence (existing behavior)" {
  TEST_COMMENT="Demo started: #9999" \
    run bash "$CHORUS_ROOT/skills/demo/gates/done-gate.sh" 9999 kade
  [ "$status" -eq 0 ]
}

@test "done-gate refuses when no evidence present" {
  TEST_COMMENT="just an unrelated note" \
    run bash "$CHORUS_ROOT/skills/demo/gates/done-gate.sh" 9999 kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"no demo evidence"* ]]
}

@test "done-gate accepts brief file (existing behavior)" {
  touch "$CHORUS_ROOT/roles/wren/briefs/2026-05-06-demo-9999.md"
  TEST_COMMENT="just an unrelated note" \
    run bash "$CHORUS_ROOT/skills/demo/gates/done-gate.sh" 9999 kade
  [ "$status" -eq 0 ]
}

@test "done-gate skips type:chore cards" {
  TEST_DOMAINS="domain:chorus, type:chore" \
  TEST_COMMENT="just an unrelated note" \
    run bash "$CHORUS_ROOT/skills/demo/gates/done-gate.sh" 9999 kade
  [ "$status" -eq 0 ]
}
