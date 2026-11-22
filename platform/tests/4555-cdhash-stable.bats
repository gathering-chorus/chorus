#!/usr/bin/env bats
# @test-type: unit — builds chorus-inject (no dependencies) offline in scratch folders.
# @domain: builds — unchanged source rebuilt in the shared folder keeps its cdhash (#4555)
#
# #4555 AC3 — macOS TCC binds AppleEvents to the cdhash of chorus-inject (#2734).
# Measured 11-20 05:08: the binary's bytes depend on the path of the folder it is
# built in (out/x and out/longer-name/y differ). So the folder must stay put between
# builds: every build of a checkout goes to one fixed folder, <checkout>/target/<account>,
# and a rebuild there of unchanged source gives the same bytes, so the same cdhash once
# signed with the same identity (no signing identity is needed in a test). Moving to that
# folder changes inject's cdhash once; the card's After the land section says so.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# Build the scratch copy of chorus-inject into folder $1 from clean; print the binary's sha.
build_sha() {
  local tgt="$1"
  rm -rf "$tgt"
  (cd "$S/inject" && CARGO_TARGET_DIR="$tgt" cargo build --release --offline -q) || return 1
  shasum -a 256 "$tgt/release/chorus-inject" | cut -d' ' -f1
}

setup() {
  command -v cargo >/dev/null || { echo "cargo not on PATH"; return 1; }
  S="$BATS_TEST_TMPDIR"
  # #4915 — never copy target/: canonical still holds a pre-workspace chorus-inject/target
  # whose lock files another account owns, and cp -R failed on them every night (11-21).
  mkdir -p "$S/inject"
  (cd "$ROOT/platform/services/chorus-inject" && tar -cf - --exclude ./target .) | (cd "$S/inject" && tar -xf -)
  cp "$ROOT/rust-toolchain.toml" "$S/inject/"
}

@test "4555 AC3: unchanged source rebuilt from clean in the same checkout folder gives the same binary" {
  a="$(build_sha "$S/checkout/target/acct")" || return 1
  b="$(build_sha "$S/checkout/target/acct")" || return 1
  [ -n "$a" ] || return 1
  [ "$a" = "$b" ] || { echo "a clean rebuild changed the bytes: $a vs $b"; return 1; }
}

@test "4555 AC3 NEGATIVE PROOF: a one-line source change in the same folder gives a different binary" {
  a="$(build_sha "$S/checkout/target/acct")" || return 1
  f="$(ls "$S/inject/src/"*.rs | head -1)"
  printf '\n#[allow(dead_code)]\nfn cdhash_probe_4555() -> u64 { 0x4555_4555 }\n#[used]\nstatic CDHASH_PROBE_4555: fn() -> u64 = cdhash_probe_4555;\n' >> "$f"
  b="$(build_sha "$S/checkout/target/acct")" || return 1
  [ "$a" != "$b" ] || { echo "a source change left the bytes the same: this check cannot see a change"; return 1; }
}

@test "4555 AC3: the folder is part of the fingerprint, which is why it must stay fixed" {
  a="$(build_sha "$S/out/x")" || return 1
  b="$(build_sha "$S/out/longer-name/y")" || return 1
  [ "$a" != "$b" ] || { echo "two folders gave the same bytes: the fixed-folder rule would be unneeded; revisit the After the land TCC step"; return 1; }
}
