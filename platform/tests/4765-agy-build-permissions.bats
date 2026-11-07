#!/usr/bin/env bats
# @domain: builds — the product domain this suite guards (#4538)
# @test-type: integration

setup() {
  load test_helper
  export WORKTREE="${CHORUS_ROOT:-.}"
}

@test "worktree services have group-writable permissions for chorus" {
  run test -w "$WORKTREE"
  [ "$status" -eq 0 ]
}

# #4915 (Silas, Wren) — was "the canonical root carries an inherited group:chorus ACL".
# The root is Jeff's and a root ACL opens all of canonical to every role, so the
# guarded thing is narrower: each account builds in its own target dir (#4772,
# build-signed since #4915), and that dir is writable by the account that owns it.
@test "each account builds in its own target dir, and can write it" {
  d="$WORKTREE/platform/services/chorus-hooks/target/$(id -un)"
  mkdir -p "$d" || return 1
  touch "$d/.p" && rm "$d/.p" || return 1
}

@test "negative proof: a locked file in the build dir cannot be overwritten (the 10-30 shape)" {
  d="$BATS_TEST_TMPDIR/target/$(id -un)"; mkdir -p "$d"
  printf 'theirs\n' > "$d/bindgen.rs"; chflags uchg "$d/bindgen.rs"
  run sh -c "printf 'mine\n' > '$d/bindgen.rs'"
  chflags nouchg "$d/bindgen.rs"
  [ "$status" -ne 0 ] || return 1
}

@test "headless user can create and remove build artifacts in target dir" {
  local target_dir="$WORKTREE/platform/services/chorus-hooks/target"
  mkdir -p "$target_dir"
  run touch "$target_dir/test-permission-check"
  [ "$status" -eq 0 ]
  rm -f "$target_dir/test-permission-check"
}
