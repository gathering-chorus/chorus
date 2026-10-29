#!/usr/bin/env bats
# @test-type: contract — signal:api is local /api/chorus/search read for AC4 session backfill check
# @domain: identity — per-role account workspace parity and 5-account session backfill (#4785 Slice 2)

load test_helper

setup() {
  ROOT="$CHORUS_ROOT"
  ENV_SETUP="$ROOT/platform/scripts/chorus-env-setup.sh"
  CANONICAL_GIT="$(GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0='*' git -C "$ROOT" rev-parse --git-common-dir 2>/dev/null || true)"
  if [ -n "$CANONICAL_GIT" ] && [ -d "$CANONICAL_GIT" ]; then
    CANONICAL="$(cd "$CANONICAL_GIT/.." && pwd -P)"
  else
    CANONICAL="$ROOT"
  fi
}

@test "AC1: chorus-env-setup.sh and chorus-principal configure safe.directory=* and git status succeeds in canonical" {
  run env -u GIT_CONFIG_COUNT -u GIT_CONFIG_KEY_0 -u GIT_CONFIG_VALUE_0 \
    bash -c "source '$ENV_SETUP' && chorus_verify_git_safe_dir && printf '%s:%s:%s\n' \"\$GIT_CONFIG_COUNT\" \"\$GIT_CONFIG_KEY_0\" \"\$GIT_CONFIG_VALUE_0\" && git -C '$CANONICAL' status --porcelain=v1 >/dev/null"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"1:safe.directory:*"* ]] || return 1

  run grep -q "GIT_CONFIG_KEY_0='safe.directory'" "$ROOT/platform/services/chorus-principal/src/lib.rs"
  [ "$status" -eq 0 ]
}

@test "AC1 NEGATIVE PROOF: chorus_verify_git_safe_dir fails loudly when safe.directory=* is absent (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/no-safe-home"
  printf '[user]\n\tname = test\n' > "$BATS_TEST_TMPDIR/no-safe-home/.gitconfig"
  run env HOME="$BATS_TEST_TMPDIR/no-safe-home" GIT_CONFIG_NOSYSTEM=1 \
    bash -c "source '$ENV_SETUP' && unset GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 && chorus_verify_git_safe_dir"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"missing safe.directory"* ]] || return 1
}

@test "AC2: nudge_send.rs, buzz_reply.rs, and chorus-env-setup.sh resolve per-role CHORUS_OUTBOX_DIR and drain cleanly" {
  mkdir -p "$BATS_TEST_TMPDIR/outbox/borg-queen"
  chmod 700 "$BATS_TEST_TMPDIR/outbox/borg-queen"
  run env CHORUS_OUTBOX_DIR="$BATS_TEST_TMPDIR/outbox" \
    bash -c "source '$ENV_SETUP' && chorus_verify_role_outbox \"\$CHORUS_OUTBOX_DIR/borg-queen\""
  echo "$output"
  [ "$status" -eq 0 ]

  run grep -q 'CHORUS_OUTBOX_DIR' "$ROOT/platform/services/chorus-hooks/src/nudge_send.rs"
  [ "$status" -eq 0 ]
  run grep -q 'CHORUS_IDENTITY_DIR' "$ROOT/platform/services/chorus-hooks/src/hooks/buzz_reply.rs"
  [ "$status" -eq 0 ]
}

@test "AC2 NEGATIVE PROOF: chorus_verify_role_outbox fails loudly on an unreadable (0000) outbox directory (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/bad-outbox"
  chmod 000 "$BATS_TEST_TMPDIR/bad-outbox"
  run bash -c "source '$ENV_SETUP' && chorus_verify_role_outbox '$BATS_TEST_TMPDIR/bad-outbox'"
  chmod 700 "$BATS_TEST_TMPDIR/bad-outbox"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"unreadable"* ]] || return 1
}

@test "AC3: chorus_sync_role_skills links all canonical skills and settings.local.json + ENABLE_CLAUDEAI_MCP_SERVERS=false are configured" {
  mkdir -p "$BATS_TEST_TMPDIR/skills-target"
  run bash -c "source '$ENV_SETUP' && chorus_sync_role_skills '$BATS_TEST_TMPDIR/skills-target' && chorus_verify_role_skills '$BATS_TEST_TMPDIR/skills-target' && printf 'ENABLE_CLAUDEAI_MCP_SERVERS=%s\n' \"\$ENABLE_CLAUDEAI_MCP_SERVERS\""
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ENABLE_CLAUDEAI_MCP_SERVERS=false"* ]] || return 1

  for role in wren silas kade; do
    run bash -c "source '$ENV_SETUP' && chorus_verify_role_skills '$ROOT/roles/$role/.claude/skills'"
    echo "$output"
    [ "$status" -eq 0 ]
    run grep -q '"grafana"' "$CANONICAL/roles/$role/.claude/settings.local.json"
    [ "$status" -eq 0 ]
  done

  run grep -q "ENABLE_CLAUDEAI_MCP_SERVERS='false'" "$ROOT/platform/services/chorus-principal/src/lib.rs"
  [ "$status" -eq 0 ]
}

@test "AC3 NEGATIVE PROOF: chorus_verify_role_skills fails loudly when a canonical skill is missing (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/incomplete-skills"
  run bash -c "source '$ENV_SETUP' && chorus_sync_role_skills '$BATS_TEST_TMPDIR/incomplete-skills' && rm -rf '$BATS_TEST_TMPDIR/incomplete-skills/cw' && chorus_verify_role_skills '$BATS_TEST_TMPDIR/incomplete-skills'"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"missing skill: cw"* ]] || return 1
}

@test "AC4: shim.rs extracts conversation UUID for Antigravity transcripts, provides idempotent session-backfill under the caller's token, and borg-queen welcome search returns 2026-10-23" {
  run grep -q 'antigravity_session_id' "$ROOT/platform/services/chorus-hooks/src/shim.rs"
  [ "$status" -eq 0 ]

  run grep -q '"session-backfill" =>' "$ROOT/platform/services/chorus-hooks/src/shim.rs"
  [ "$status" -eq 0 ]
  run grep -q 'extract_backfill_turns' "$ROOT/platform/services/chorus-hooks/src/shim.rs"
  [ "$status" -eq 0 ]
  run grep -q '{role}:{session_id}:{turn_idx}' "$ROOT/platform/services/chorus-hooks/src/shim.rs"
  [ "$status" -eq 0 ]

  run curl -sf "http://localhost:3340/api/chorus/search?q=welcome&channel=session:borg-queen&order=oldest&limit=1"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"2026-10-23"* ]] || return 1

  # Idempotency check: re-pushing the same source_id turn under caller's token inserts 0 duplicate rows
  local role token
  role="${CHORUS_ROLE:-}"
  if [ -z "$role" ]; then
    local me
    me="$(id -un 2>/dev/null || whoami)"
    case "$me" in
      chorus-*) role="${me#chorus-}" ;;
      *) role="borg-queen" ;;
    esac
  fi
  token="$("$ROOT/platform/scripts/chorus-identity-token" "$role")"
  run curl -sf -X POST "http://localhost:3340/api/chorus/session/turns" \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"role\":\"$role\",\"source\":\"antigravity\",\"session_id\":\"c20fed59-2575-431e-92d5-33b4939ba2e4\",\"turns\":[{\"author\":\"user\",\"content\":\"<USER_REQUEST>\\nhi $role welcome!\\n</USER_REQUEST>\",\"timestamp\":\"2026-10-23T13:04:23Z\",\"source\":\"antigravity\",\"source_id\":\"antigravity:c20fed59-2575-431e-92d5-33b4939ba2e4:step0\",\"session_id\":\"c20fed59-2575-431e-92d5-33b4939ba2e4\"}]}"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"inserted":0'* ]] || return 1
}

@test "AC4 NEGATIVE PROOF: session ID extractor rejects bare transcript stem and cross-role session turn push is refused (#4779, #3734)" {
  run python3 - "$ROOT/platform/services/chorus-hooks/src/shim.rs" <<'PY'
import pathlib, sys
src = pathlib.Path(sys.argv[1]).read_text()
if "fn antigravity_session_id" not in src:
    print("missing antigravity_session_id in shim.rs")
    sys.exit(1)
if '!= "transcript"' not in src and '!= Some("transcript")' not in src and '.system_generated/logs/' not in src:
    print("antigravity_session_id does not guard against bare 'transcript'")
    sys.exit(1)
print("ok")
PY
  echo "$output"
  [ "$status" -eq 0 ]

  # Negative proof (#4779, #3734): caller's token pushing a turn claiming a different role is refused with HTTP 403
  local role token other_role http_code
  role="${CHORUS_ROLE:-}"
  if [ -z "$role" ]; then
    local me
    me="$(id -un 2>/dev/null || whoami)"
    case "$me" in
      chorus-*) role="${me#chorus-}" ;;
      *) role="borg-queen" ;;
    esac
  fi
  [ "$role" = "wren" ] && other_role="silas" || other_role="wren"
  token="$("$ROOT/platform/scripts/chorus-identity-token" "$role")"
  http_code="$(curl -s -o "$BATS_TEST_TMPDIR/refusal.json" -w "%{http_code}" -X POST "http://localhost:3340/api/chorus/session/turns" \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"role\":\"$other_role\",\"source\":\"antigravity\",\"session_id\":\"neg-test\",\"turns\":[{\"author\":\"assistant\",\"content\":\"forged turn\"}]}")"
  echo "http_code=$http_code body=$(cat "$BATS_TEST_TMPDIR/refusal.json")"
  [ "$http_code" = "403" ] || [ "$http_code" = "401" ]
  grep -Eq 'not-your-lane|authn-missing' "$BATS_TEST_TMPDIR/refusal.json"
}
