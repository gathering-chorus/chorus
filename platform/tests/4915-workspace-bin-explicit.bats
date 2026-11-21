#!/usr/bin/env bats
# @test-type: unit — the workspace binary lookup on fixture trees and a stand-in cargo; no build
# @domain: tests
# @card: #4915 · owner: kade
# What Jeff sees: a test runs the binary its caller asked for, built where the account's builds
# go. Code review of e6353088c: a newer build elsewhere outranked an explicit CARGO_TARGET_DIR,
# and the build-if-missing step built debug only, into the shared target/.
# Covers: platform/scripts/lib/workspace-bin.sh

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LIB="$ROOT/platform/scripts/lib/workspace-bin.sh"
. "$LIB"

mk() { mkdir -p "$(dirname "$1")"; printf '#!/bin/sh\n' > "$1"; chmod +x "$1"; touch -t "$2" "$1"; }

@test "#4915 round 5: an explicit CARGO_TARGET_DIR that holds the binary is the answer" {
  local r="$BATS_TEST_TMPDIR/r"
  mk "$r/explicit/debug/x" 202611200300
  mk "$r/target/debug/x"   202611211200
  [ "$(CARGO_TARGET_DIR="$r/explicit" workspace_bin "$r" debug x)" = "$r/explicit/debug/x" ]
}

# NEGATIVE PROOF: the newer build outside the explicit folder is not picked
@test "#4915 round 5 NEGATIVE: a newer build outside the explicit CARGO_TARGET_DIR is not the answer" {
  local r="$BATS_TEST_TMPDIR/r2"
  mk "$r/explicit/debug/x" 202611200300
  mk "$r/target/$(id -un)/debug/x" 202611211200
  [ "$(CARGO_TARGET_DIR="$r/explicit" workspace_bin "$r" debug x)" != "$r/target/$(id -un)/debug/x" ]
}

stand_in_cargo() {  # a cargo that records its CARGO_TARGET_DIR and args, then builds z there
  local bin="$BATS_TEST_TMPDIR/bin"; mkdir -p "$bin"
  cat > "$bin/cargo" <<'SH'
#!/bin/sh
echo "$CARGO_TARGET_DIR $*" >> "$CARGO_LOG"
p=debug; for a in "$@"; do [ "$a" = --release ] && p=release; done
mkdir -p "$CARGO_TARGET_DIR/$p" && printf '#!/bin/sh\n' > "$CARGO_TARGET_DIR/$p/z" && chmod +x "$CARGO_TARGET_DIR/$p/z"
SH
  chmod +x "$bin/cargo"; printf '%s' "$bin"
}

@test "#4915 round 5: a missing binary is built into the account's folder, never the shared target/" {
  local r="$BATS_TEST_TMPDIR/r3" bin; bin="$(stand_in_cargo)"; mkdir -p "$r"
  run env -u CARGO_TARGET_DIR PATH="$bin:$PATH" CARGO_LOG="$BATS_TEST_TMPDIR/log" bash -c ". '$LIB'; workspace_bin_built '$r' z z debug"
  [ "$status" -eq 0 ] && [ "$output" = "$r/target/$(id -un)/debug/z" ] || { echo "$output"; return 1; }
  grep -q "^$r/target/$(id -un) " "$BATS_TEST_TMPDIR/log" || { cat "$BATS_TEST_TMPDIR/log"; return 1; }
  [ ! -e "$r/target/debug/z" ]
}

@test "#4915 round 5: the asked profile is built (release passes --release)" {
  local r="$BATS_TEST_TMPDIR/r4" bin; bin="$(stand_in_cargo)"; mkdir -p "$r"
  run env -u CARGO_TARGET_DIR PATH="$bin:$PATH" CARGO_LOG="$BATS_TEST_TMPDIR/log4" bash -c ". '$LIB'; workspace_bin_built '$r' z z release"
  [ "$output" = "$r/target/$(id -un)/release/z" ] || { echo "$output"; return 1; }
  grep -q -- "--release" "$BATS_TEST_TMPDIR/log4"
}
