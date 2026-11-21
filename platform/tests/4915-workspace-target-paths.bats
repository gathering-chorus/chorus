#!/usr/bin/env bats
# @test-type: unit — scans the tree's test and script files; no build, no services
# @domain: tests
# @card: #4915 · owner: kade
# What Jeff sees: the nightly stops going red on tests that look for a binary where no build puts it.
# #4555 (11-20) made one cargo workspace with one target/ at the repo root; 21 test and script files
# still named a crate's own platform/services/<crate>/target/, and the 11-21 nightly had 5 suites red
# on it (4773 x3, 4915-unknown-domain, 4039). The rule: a test reads a built binary from the
# workspace target ($ROOT/target or CARGO_TARGET_DIR), never from a crate's own target/.
# NEGATIVE: a planted test naming a crate's own target/debug is caught by name.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# files that name a crate's own target/ on purpose: a permission test of that folder, a fixture
# tree, and two suites that set their own CARGO_TARGET_DIR to it and read it back consistently
ALLOWED='platform/tests/4765-agy-build-permissions.bats|platform/tests/4915-verb-drift-retired.bats|platform/tests/4960-policy-schedule.bats|platform/tests/inject-lookup-timing.bats|platform/tests/4915-workspace-target-paths.bats|platform/scripts/test-compound-loop.sh'

old_paths() {  # repo root → each "file:line" naming a crate's own target dir, outside the allowed files
  (cd "$1" && grep -rnE 'services/[a-z0-9_-]+/target(/(debug|release)|[}"'"'"' ]|$)' platform/tests platform/scripts skills proving/scripts 2>/dev/null) \
    | grep -vE "^($ALLOWED):" || true
}

@test "#4915: no test or script reads a binary from a crate's own target/ (the workspace builds to one root target/)" {
  run old_paths "$ROOT"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "#4915 NEGATIVE: a test naming a crate's own target/debug is caught by name" {
  local t="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$t/platform/tests" "$t/platform/scripts" "$t/skills" "$t/proving/scripts"
  printf 'BIN="$ROOT/platform/services/borg-prove/target/debug/borg-prove"\n' > "$t/platform/tests/old.bats"
  printf 'D="${CARGO_TARGET_DIR:-$ROOT/platform/services/chorus-hooks/target}/debug/x"\n' > "$t/platform/tests/old2.bats"
  printf 'BIN="$ROOT/target/debug/borg-prove"\n' > "$t/platform/tests/new.bats"
  run old_paths "$t"
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 2 ] || { echo "$output"; return 1; }
  [[ "$output" == *"platform/tests/old.bats:1"* && "$output" == *"platform/tests/old2.bats:1"* ]] || { echo "$output"; return 1; }
}
