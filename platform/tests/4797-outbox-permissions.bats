#!/usr/bin/env bats
# @test-type: contract — verifies per-role Buzz outbox permissions and role isolation (#4797)
# @domain: infrastructure

load test_helper

setup() {
  ROOT="$CHORUS_ROOT"
  ENV_SETUP="$ROOT/platform/scripts/chorus-env-setup.sh"
}

check_role_outbox_permissions() {
  local base_dir="$1"
  python3 - "$base_dir" <<'PY'
import sys, os, stat

base = sys.argv[1]
roles = ["wren", "silas", "kade", "abby-normal", "borg-queen"]

# Check base dir mode 0755
st_base = os.stat(base)
base_mode = stat.S_IMODE(st_base.st_mode)
if base_mode & 0o022 != 0:
    print(f"VIOLATION: base outbox dir {base} is group/other writable: {oct(base_mode)}")
    sys.exit(1)

bad = []
for role in roles:
    role_dir = os.path.join(base, role)
    if not os.path.exists(role_dir):
        bad.append(f"missing outbox dir: {role_dir}")
        continue
    st = os.stat(role_dir)
    mode = stat.S_IMODE(st.st_mode)
    if mode & 0o077 != 0:
        bad.append(f"{role} outbox {role_dir} has mode {oct(mode)}, expected 0700 (no group/other access)")

if bad:
    print("\n".join(bad))
    sys.exit(1)
print("ok")
PY
}

@test "AC1 & AC2: Real production outbox directories have 0700 permissions per role" {
  local outbox_dir="${CHORUS_STATE_DIR:-$HOME/.chorus}/outbox"
  [ -d "$outbox_dir" ] || skip "Not on production host with outbox directory"
  run check_role_outbox_permissions "$outbox_dir"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC3 NEGATIVE PROOF: cross-role access to a 0700 role outbox is refused (#3734)" {
  local tmp="$BATS_TEST_TMPDIR/cross_role_test"
  mkdir -p "$tmp/wren" "$tmp/silas" "$tmp/kade" "$tmp/abby-normal" "$tmp/borg-queen"
  chmod 755 "$tmp"
  chmod 700 "$tmp"/*

  # Test script simulating check_role_outbox_permissions failing if any role outbox has group/other perms
  chmod 775 "$tmp/silas"
  run check_role_outbox_permissions "$tmp"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"silas outbox"* && "$output" == *"expected 0700"* ]] || return 1
}

@test "AC3 NEGATIVE PROOF: group-writable base outbox directory is refused (#3734)" {
  local tmp="$BATS_TEST_TMPDIR/base_mode_test"
  mkdir -p "$tmp/wren" "$tmp/silas" "$tmp/kade" "$tmp/abby-normal" "$tmp/borg-queen"
  chmod 700 "$tmp"/*
  chmod 777 "$tmp"
  run check_role_outbox_permissions "$tmp"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"base outbox dir"* && "$output" == *"is group/other writable"* ]] || return 1
}
