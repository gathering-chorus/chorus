#!/usr/bin/env bats
# @test-type: integration — werk-pull/werk-build node_modules trees and the npm guard, on fixture werks
# @domain: werk
#
# #4810. A werk's node_modules used to be a symlink into canonical, or nothing.
# Nothing made local proof go UNMEASURED (#4552, #4787 run 6, #4809 run 1). The
# symlink let `npm ci` in the werk delete canonical's live tree: 10-21
# (wren-4672/pulse) and 10-30 13:54:37 (cards — the cards CLI broke for every
# role). These checks fail on a missing crate or test, never skip (#3734).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SVC="$ROOT/platform/services"
}

# Runs one crate's tests matching a filter; fails when none ran (a renamed test
# must not pass vacuously).
crate_tests() {
  local crate="$1" filter="$2"; shift 2
  # the guard logs its refusal to the spine; a test brings its own (#3528)
  CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/chorus.log" run nice cargo test -q --manifest-path "$SVC/$crate/Cargo.toml" "$@" "$filter"
  echo "$output" | tail -5
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE 'test result: ok\. [1-9][0-9]* passed' || return 1
}

@test "AC1: werk-pull clones canonical's tree into each package, stamped, never a symlink" {
  crate_tests werk-pull materialize --test units
}

@test "AC1: werk-build replaces a symlink or partial with canonical's tree, and keeps a stamped one" {
  crate_tests werk-build ensure_node_modules --test units
  crate_tests werk-build a_werk_with_its_own_lock --test lock_install_4511
}

@test "AC1: neither verb creates a node_modules symlink any more" {
  run grep -n 'unix::fs::symlink(' "$SVC/werk-pull/src/lib.rs" "$SVC/werk-build/src/lib.rs"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
}

@test "AC2 negative proof: unguarded npm ci through a symlinked node_modules empties the target" {
  command -v npm >/dev/null || { echo "npm not on PATH"; return 1; }
  canon="$BATS_TEST_TMPDIR/canon/node_modules"
  werk="$BATS_TEST_TMPDIR/werk"
  mkdir -p "$canon" "$werk"
  echo live > "$canon/marker"
  printf '{"name":"fx","version":"1.0.0"}\n' > "$werk/package.json"
  printf '{"name":"fx","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"fx","version":"1.0.0"}}}\n' > "$werk/package-lock.json"
  ln -s "$canon" "$werk/node_modules"
  (cd "$werk" && npm ci --offline --no-audit --no-fund >/dev/null 2>&1) || true
  # the state the guard exists to prevent is real: canonical's file is gone
  [ ! -e "$canon/marker" ] || { echo "npm ci did not reach through the link; this fixture proves nothing"; return 1; }
}

# AC2's guard itself is proven in its crate: chorus-hooks infra_guardrails
# test_4810_* (deny naming the target, cd/--prefix forms, and the separating
# allow half), which werk-test runs as cargo-test:chorus-hooks. Compiling
# chorus-hooks a second time here only doubled the load (run 2, 2026-10-31).
