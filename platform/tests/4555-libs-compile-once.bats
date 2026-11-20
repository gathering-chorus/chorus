#!/usr/bin/env bats
# @test-type: unit — reads the repo's manifests and builds two tiny scratch crates offline.
# @domain: builds — shared libraries compile once across every service (#4555)
#
# #4555 AC2 — measured 11-20 04:29: in one shared target folder, chorus-runner then
# chorus-agent still compiled 130 and 120 libraries, axum included, because each
# crate locked its own versions (tokio 1.52.1 / 1.53.1 / 1.53.2). One workspace
# means one Cargo.lock, so a library has one version and one build; measured
# 04:45: the whole workspace in one folder, tokio compiled once, 1.0 GB.
#
# chorus-inject stays outside the workspace (its own lock, lto+strip).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# Prints what breaks "one workspace, one lock" in the tree at $1; empty when sound.
workspace_faults() {
  local r="$1" d
  grep -q '^\[workspace\]' "$r/Cargo.toml" 2>/dev/null || echo "no [workspace] in the root Cargo.toml"
  [ -f "$r/Cargo.lock" ] || echo "no root Cargo.lock"
  # every crate the workspace means to hold: services, athena-gen, product and domain APIs
  for d in "$r"/platform/services/*/ "$r"/designing/services/*/ "$r"/directing/products/*/api/ "$r"/shaping/domains/*/api/; do
    d="${d%/}"; [ -f "$d/Cargo.toml" ] || continue
    rel="${d#$r/}"
    case "$rel" in platform/services/chorus-inject) continue ;; esac
    [ -f "$d/Cargo.lock" ] && echo "own lock: $rel/Cargo.lock"
    grep -qE "\"($rel|${rel%/*/api}/\*/api)\"" "$r/Cargo.toml" || echo "not a member: $rel"
  done
  true
}

@test "4555 AC2: the repo root is one workspace with one Cargo.lock; no member keeps its own" {
  out="$(workspace_faults "$ROOT")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "4555 AC2 NEGATIVE PROOF: a member with its own lock, or a crate left out, is named" {
  t="$BATS_TEST_TMPDIR/tree"; mkdir -p "$t/platform/services/a" "$t/platform/services/b"
  printf '[workspace]\nmembers = ["platform/services/a"]\n' > "$t/Cargo.toml"; : > "$t/Cargo.lock"
  printf '[package]\nname = "a"\n' > "$t/platform/services/a/Cargo.toml"; : > "$t/platform/services/a/Cargo.lock"
  printf '[package]\nname = "b"\n' > "$t/platform/services/b/Cargo.toml"
  mkdir -p "$t/shaping/domains/d/api"; printf '[package]\nname = "d"\n' > "$t/shaping/domains/d/api/Cargo.toml"
  : > "$t/shaping/domains/d/api/Cargo.lock"
  out="$(workspace_faults "$t")"
  echo "$out" | grep -q 'own lock: platform/services/a/Cargo.lock' || return 1
  echo "$out" | grep -q 'not a member: platform/services/b' || return 1
  echo "$out" | grep -q 'own lock: shaping/domains/d/api/Cargo.lock' || return 1   # outside platform/services too
  echo "$out" | grep -q 'not a member: shaping/domains/d/api' || return 1
}

@test "4555 AC2: a glob member (shaping/domains/*/api) counts as membership" {
  t="$BATS_TEST_TMPDIR/glob"; mkdir -p "$t/shaping/domains/d/api"
  printf '[workspace]\nmembers = ["shaping/domains/*/api"]\n' > "$t/Cargo.toml"; : > "$t/Cargo.lock"
  printf '[package]\nname = "d"\n' > "$t/shaping/domains/d/api/Cargo.toml"
  out="$(workspace_faults "$t")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

# Two binaries that use one library. $1 = workspace|separate, $2 = broken breaks b. Builds a then b and
# prints how many times b's build compiled the library (0 = shared). A failed build
# of b fails the function: no Compiling lines from a build that broke is not "shared".
compiles_of_lib_for_b() {
  local w="$BATS_TEST_TMPDIR/$1"; mkdir -p "$w/lib/src" "$w/a/src" "$w/b/src"
  cp "$ROOT/rust-toolchain.toml" "$w/"   # the repo's pinned toolchain, outside the repo too
  printf '[package]\nname = "sharedlib"\nversion = "0.1.0"\nedition = "2021"\n' > "$w/lib/Cargo.toml"
  echo 'pub fn n() -> u32 { 7 }' > "$w/lib/src/lib.rs"
  for c in a b; do
    printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nsharedlib = { path = "../lib" }\n' "$c" > "$w/$c/Cargo.toml"
    echo 'fn main() { println!("{}", sharedlib::n()); }' > "$w/$c/src/main.rs"
  done
  [ "${2:-}" = broken ] && echo 'fn main() { this does not compile }' > "$w/b/src/main.rs"
  if [ "$1" = workspace ]; then
    printf '[workspace]\nresolver = "2"\nmembers = ["lib", "a", "b"]\n' > "$w/Cargo.toml"
    (cd "$w/a" && CARGO_TARGET_DIR="$w/target/acct" cargo build --offline -q) || return 1
    out="$(cd "$w/b" && CARGO_TARGET_DIR="$w/target/acct" cargo build --offline -v 2>&1)" || { echo "build of b failed: $out" >&2; return 1; }
    [ -x "$w/target/acct/debug/b" ] || { echo "b built no binary" >&2; return 1; }
  else
    (cd "$w/a" && CARGO_TARGET_DIR="$w/a/target" cargo build --offline -q) || return 1
    out="$(cd "$w/b" && CARGO_TARGET_DIR="$w/b/target" cargo build --offline -v 2>&1)" || { echo "build of b failed: $out" >&2; return 1; }
    [ -x "$w/b/target/debug/b" ] || { echo "b built no binary" >&2; return 1; }
  fi
  printf '%s\n' "$out" | grep -c 'Compiling sharedlib' || true
}

@test "4555 AC2: in the workspace folder, the second binary does not compile the library again" {
  command -v cargo >/dev/null || { echo "cargo not on PATH"; return 1; }
  n="$(compiles_of_lib_for_b workspace)" || return 1
  [ "$n" = 0 ] || { echo "b compiled sharedlib $n time(s)"; return 1; }
}

@test "4555 AC2 NEGATIVE PROOF: the per-crate layout compiles the library again for the second binary" {
  command -v cargo >/dev/null || { echo "cargo not on PATH"; return 1; }
  n="$(compiles_of_lib_for_b separate)" || return 1
  [ "$n" -ge 1 ] || { echo "per-crate layout reused the library ($n): this check cannot tell the layouts apart"; return 1; }
}

@test "4555 AC2 NEGATIVE PROOF: a build of b that breaks is a failure, never 0 compiles" {
  command -v cargo >/dev/null || { echo "cargo not on PATH"; return 1; }
  run compiles_of_lib_for_b workspace broken
  [ "$status" -ne 0 ] || { echo "a broken build read as '$output' compiles: the AC2 count cannot tell a failure from sharing"; return 1; }
}
