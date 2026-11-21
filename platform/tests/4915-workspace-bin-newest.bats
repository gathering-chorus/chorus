#!/usr/bin/env bats
# @test-type: unit — the workspace binary lookup on fixture trees, and a scan of the tree's scripts; no build
# @domain: tests
# @card: #4915 · owner: kade
# What Jeff sees: a test runs the binary that was just built, never an older copy that happens to sit
# first in line, and no script reads a crate's own target/ under any name (slice 3, the code review
# of round c4a91f3bf: first-in-line could answer a stale build; 4773 suites looked for debug only;
# the guard missed $CRATE/target and the shaping/, designing/, directing/ trees).
# Covers: platform/scripts/lib/workspace-bin.sh

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
. "$ROOT/platform/scripts/lib/workspace-bin.sh"

mk() { mkdir -p "$(dirname "$1")"; printf '#!/bin/sh\n' > "$1"; chmod +x "$1"; touch -t "$2" "$1"; }

@test "#4915 slice 3: the newest build is answered, wherever it sits in line" {
  local r="$BATS_TEST_TMPDIR/r"
  mk "$r/target/$(id -un)/debug/x" 202611200300   # the account's build, first in line, older
  mk "$r/target/debug/x"            202611211200   # a hand build, last in line, newer
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" debug x)" = "$r/target/debug/x" ]
}

# NEGATIVE PROOF: the old first-in-line answer is exactly the stale one
@test "#4915 slice 3 NEGATIVE: the older build first in line is not the answer" {
  local r="$BATS_TEST_TMPDIR/r2"
  mk "$r/target/$(id -un)/debug/x" 202611200300
  mk "$r/target/debug/x"            202611211200
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" debug x)" != "$r/target/$(id -un)/debug/x" ]
}

@test "#4915 slice 3: 'any' takes the newest of both profiles; nothing built names the pipeline's place" {
  local r="$BATS_TEST_TMPDIR/r3"
  mk "$r/target/$(id -un)/debug/y"   202611200300
  mk "$r/target/$(id -un)/release/y" 202611211200
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" any y)" = "$r/target/$(id -un)/release/y" ]
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" any nope)" = "$r/target/$(id -un)/debug/nope" ]
}

@test "#4915 slice 3: workspace_bin_built builds a missing binary once and answers it; a failed build answers nothing" {
  local r="$BATS_TEST_TMPDIR/r4" bin="$BATS_TEST_TMPDIR/bin"
  mkdir -p "$r" "$bin"
  # a stand-in cargo: builds z into CARGO_TARGET_DIR/debug, or fails for package "broken"
  printf '#!/bin/sh\n[ "$4" = broken ] && exit 101\nmkdir -p "$CARGO_TARGET_DIR/debug" && printf "#!/bin/sh\\n" > "$CARGO_TARGET_DIR/debug/z" && chmod +x "$CARGO_TARGET_DIR/debug/z"\n' > "$bin/cargo"
  chmod +x "$bin/cargo"
  run env PATH="$bin:$PATH" CARGO_TARGET_DIR="$r/t" bash -c ". '$ROOT/platform/scripts/lib/workspace-bin.sh'; workspace_bin_built '$r' z z"
  [ "$status" -eq 0 ] && [ "$output" = "$r/t/debug/z" ] || { echo "$output"; return 1; }
  run env PATH="$bin:$PATH" CARGO_TARGET_DIR="$r/t2" bash -c ". '$ROOT/platform/scripts/lib/workspace-bin.sh'; workspace_bin_built '$r' w broken"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
}

ALLOWED='platform/tests/4765-agy-build-permissions.bats|platform/tests/4915-verb-drift-retired.bats|platform/tests/4915-workspace-target-paths.bats|platform/tests/4915-workspace-bin-newest.bats|platform/scripts/test-compound-loop.sh'
crate_own() {  # repo root → "file:line" naming a crate's own target/, under services/, api/ or $CRATE
  (cd "$1" && grep -rnE '(services/[a-z0-9_-]+|/api|\$\{?[Cc][Rr][Aa][Tt][Ee]\}?)/target(/(debug|release)|[}"'"'"' ]|$)' \
     platform/tests platform/scripts skills proving shaping designing directing \
     --include='*.bats' --include='*.sh' --include='*.py' --include='*.plist' --include='*.cjs' 2>/dev/null) \
    | grep -vE "^($ALLOWED):" || true
}

@test "#4915 slice 3: no script anywhere reads a crate's own target/ (services/, api/ or \$CRATE)" {
  run crate_own "$ROOT"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "#4915 slice 3 NEGATIVE: a planted \$CRATE/target path in shaping/ is caught" {
  local t="$BATS_TEST_TMPDIR/tree"; mkdir -p "$t/shaping/domains/x/api/tests"
  printf 'for c in "$CRATE/target/release/x"; do :; done\n' > "$t/shaping/domains/x/api/tests/old.bats"
  run crate_own "$t"
  [[ "$output" == *"shaping/domains/x/api/tests/old.bats:1"* ]] || { echo "$output"; return 1; }
}
