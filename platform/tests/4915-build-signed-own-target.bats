#!/usr/bin/env bats
# @test-type: unit — build-signed.sh over a stub cargo/codesign; no real build, no install
# @domain: cicd
# @card: #4915 · owner: kade
# build-signed.sh builds into the caller's own target dir (#4772's rule; Silas 11-06).
# The shared canonical target/ held a chorus-borg-queen 0644 libsqlite3-sys
# out/bindgen.rs (10-30); every other account's build exited 101 writing over it,
# and chorus-inject-signed-stable + test-build-invariance went red each night.
# The stub cargo does what that build script does: write out/bindgen.rs under
# $CARGO_TARGET_DIR, then the binary. A locked file there is the 10-30 state.
# #4555: the repo root is one Cargo workspace, so the default folder is the
# checkout's, <checkout>/target/<account>, in a werk and in canonical alike.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

setup() {
  S="$(cd "$BATS_TEST_TMPDIR" && pwd -P)"; mkdir -p "$S/bin" "$S/crate"   # git reports the real path
  git -C "$S" init -q   # #4555: build-signed finds the checkout root from the crate
  cat > "$S/bin/cargo" <<'EOF'
#!/bin/bash
echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR" >> "$STUB_LOG"
out="$CARGO_TARGET_DIR/release/build/libsqlite3-sys-x/out"
mkdir -p "$out" || exit 101
printf 'bindings\n' > "$out/bindgen.rs" || { echo "error: failed to run custom build command for libsqlite3-sys" >&2; exit 101; }
printf '#!/bin/sh\n' > "$CARGO_TARGET_DIR/release/fake-bin" && chmod +x "$CARGO_TARGET_DIR/release/fake-bin"
EOF
  printf '#!/bin/sh\ncase "$*" in *-d*) printf "Identifier=com.test.fake\\nAuthority=test\\nCDHash=0123abcd\\n" >&2 ;; esac\nexit 0\n' > "$S/bin/codesign"
  printf '#!/bin/sh\nexit 0\n' > "$S/bin/chorus-log"
  chmod +x "$S/bin/"*
  export STUB_LOG="$S/cargo.log"
}

teardown() {
  # a locked file must be unlocked or the tmpdir cannot be removed
  [ -d "$BATS_TEST_TMPDIR/locked" ] && chflags -R nouchg "$BATS_TEST_TMPDIR/locked" 2>/dev/null || true
}

build() {
  run env PATH="$S/bin:$PATH" BUILD_SKIP_INSTALL=1 CHORUS_HOME="$S" \
    bash "$ROOT/platform/scripts/build-signed.sh" "$S/crate" com.test.fake fake-bin
}

@test "by default build-signed builds into the checkout's folder for this account" {
  unset CARGO_TARGET_DIR
  build
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  grep -qx "CARGO_TARGET_DIR=$S/target/$(id -un)" "$STUB_LOG" || { cat "$STUB_LOG"; return 1; }
  [ -x "$S/target/$(id -un)/release/fake-bin" ] || return 1
}

@test "negative proof: a target dir holding another account's locked out/bindgen.rs goes red (the 10-30 state)" {
  L="$BATS_TEST_TMPDIR/locked"; out="$L/release/build/libsqlite3-sys-x/out"
  mkdir -p "$out"; printf 'theirs\n' > "$out/bindgen.rs"; chflags uchg "$out/bindgen.rs"
  CARGO_TARGET_DIR="$L" build
  echo "$output"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"failed to run custom build command"* ]] || return 1
}

@test "the same locked shared dir does not touch a default build: the own-account build stays green" {
  L="$BATS_TEST_TMPDIR/locked/release/build/libsqlite3-sys-x/out"
  mkdir -p "$L" "$S/target"; printf 'theirs\n' > "$L/bindgen.rs"; chflags uchg "$L/bindgen.rs"
  ln -s "$BATS_TEST_TMPDIR/locked/release" "$S/target/release"   # cargo's bare shared default
  unset CARGO_TARGET_DIR
  build
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "a werk build (werk-build passes <checkout>/target/<account>) lands where werk-deploy reads it" {
  CARGO_TARGET_DIR="$S/target/$(id -un)" build
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [ -x "$S/target/$(id -un)/release/fake-bin" ] || return 1
  grep -q 'let target = account_target_dir(werk_s, &account);' "$ROOT/platform/services/werk-build/src/lib.rs" \
    || { echo "werk-build no longer passes the checkout's account folder"; return 1; }
  grep -q 'format!("{}/release/{}", account_target_dir(checkout, account), bin)' "$ROOT/platform/services/shared/target_seed.rs" \
    || { echo "built_binary no longer reads the same folder"; return 1; }
}

@test "a crate inside a werk builds into that werk's folder when the caller passes nothing (the installed werk-build)" {
  R="$S/chorus-werk/kade-1"; W="$R/platform/services/crate"; mkdir -p "$W"
  git -C "$R" init -q
  unset CARGO_TARGET_DIR
  run env PATH="$S/bin:$PATH" BUILD_SKIP_INSTALL=1 CHORUS_HOME="$S" \
    bash "$ROOT/platform/scripts/build-signed.sh" "$W" com.test.fake fake-bin
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [ -x "$R/target/$(id -un)/release/fake-bin" ] || { ls -R "$R/target" | head; return 1; }
}

@test "negative proof: a crate outside any git checkout is refused, never built somewhere unnamed" {
  N="$(mktemp -d /tmp/nogit-4555.XXXXXX)"; mkdir -p "$N/crate"
  unset CARGO_TARGET_DIR
  run env PATH="$S/bin:$PATH" BUILD_SKIP_INSTALL=1 CHORUS_HOME="$S" \
    bash "$ROOT/platform/scripts/build-signed.sh" "$N/crate" com.test.fake fake-bin
  rm -rf "$N"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"is not inside a git checkout"* ]] || return 1
}
