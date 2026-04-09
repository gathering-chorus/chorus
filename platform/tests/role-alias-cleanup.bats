#!/usr/bin/env bats
# Test: no stale role aliases in platform/scripts/
# Card: #1843 — product-manager references in scripts
# AC: all scripts use roles/wren, roles/silas, roles/kade — not old aliases

SCRIPTS_DIR="${BATS_TEST_DIRNAME}/../scripts"

@test "no scripts reference product-manager as a directory path" {
  # product-manager as a dir path (not in comments or string matching)
  # Grep for product-manager used as path component: /product-manager/ or /product-manager"
  result=$(grep -rn 'product-manager' "$SCRIPTS_DIR"/ 2>/dev/null \
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

@test "werk-init.sh maps wren to roles/wren" {
  grep -q 'roles/wren' "$SCRIPTS_DIR/werk-init.sh"
}

@test "werk-init.sh maps silas to roles/silas" {
  grep -q 'roles/silas' "$SCRIPTS_DIR/werk-init.sh"
}

@test "werk-init.sh maps kade to roles/kade" {
  grep -q 'roles/kade' "$SCRIPTS_DIR/werk-init.sh"
}
