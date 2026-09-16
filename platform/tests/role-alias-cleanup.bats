#!/usr/bin/env bats
# @test-type: fitness — greps platform/scripts for a retired directory name; no
# service, no store, no network. (#4179: the file had no declaration at all and
# the gate only grades a test file when it changes, so this went unnoticed since
# #1843 — the same shape as a check that is never run.)
#
# Test: no stale role aliases in platform/scripts/
# Card: #1843 — product-manager references in scripts
# AC: all scripts use roles/wren, roles/silas, roles/kade — not old aliases

SCRIPTS_DIR="${BATS_TEST_DIRNAME}/../scripts"

@test "no scripts reference product-manager as a directory path" {
  # The retired thing is a DIRECTORY (roles/product-manager -> roles/wren, #1843),
  # so the check looks like a path: a slash before it, and a slash or a quote
  # after it. The bare-substring version matched `hat-product-manager` in
  # platform/scripts/hats-appointments-4175.py — a hat legitimately NAMED
  # "product manager" (#4175) — and went red for two nightlies over a correct
  # string. A check hunting a path should be shaped like a path (#4179).
  result=$(grep -rnE '/product-manager(/|"|'"'"'|$)' "$SCRIPTS_DIR"/ 2>/dev/null \
    | grep -v '\.bak:' \
    | grep -v '#.*product-manager' \
    || true)
  [ -z "$result" ] || {
    echo "Stale product-manager references found:"
    echo "$result"
    false
  }
}

@test "no scripts reference architect/ as wren role directory" {
  # architect as a role directory mapping for silas
  # Only match path-like usage, not comments
  result=$(grep -rn '".*architect"' "$SCRIPTS_DIR"/ 2>/dev/null \
    | grep -iE 'role_dir|dir_name|dir_map|brief' \
    | grep -v '\.bak:' \
    || true)
  [ -z "$result" ] || {
    echo "Stale architect role-dir references found:"
    echo "$result"
    false
  }
}

@test "no scripts reference engineer/ as kade role directory" {
  result=$(grep -rn '".*engineer"' "$SCRIPTS_DIR"/ 2>/dev/null \
    | grep -iE 'role_dir|dir_name|dir_map|brief' \
    | grep -v '\.bak:' \
    || true)
  [ -z "$result" ] || {
    echo "Stale engineer role-dir references found:"
    echo "$result"
    false
  }
}

# Note: prior tests asserted werk-init.sh role mapping. Retired by #2311
# rescope — the shell wrapper was replaced by the Rust `chorus-hook-shim
# session-start <role>` subcommand. Role-dir mapping now lives in
# platform/services/chorus-hooks/src/shared/state_paths.rs (role_dir fn).
