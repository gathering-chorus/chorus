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
# Silas 11-21 10:51: a pipeline build lands in target/<account>/, a hand build in target/, so a
# test asks platform/scripts/lib/workspace-bin.sh, never a fixed target/<profile>/ path.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# files that name a crate's own target/ on purpose: a permission test of that folder, a fixture
# tree, and two suites that set their own CARGO_TARGET_DIR to it and read it back consistently
ALLOWED='platform/tests/4765-agy-build-permissions.bats|platform/tests/4915-verb-drift-retired.bats|platform/tests/4960-policy-schedule.bats|platform/tests/inject-lookup-timing.bats|platform/tests/4915-workspace-target-paths.bats|platform/scripts/test-compound-loop.sh'

old_paths() {  # repo root → each "file:line" naming a crate's own target dir, outside the allowed files
  (cd "$1" && grep -rnE 'services/[a-z0-9_-]+/target(/(debug|release)|[}"'"'"' ]|$)' platform/tests platform/scripts skills proving/scripts proving/config 2>/dev/null) \
    | grep -vE "^($ALLOWED):" || true
}

@test "#4915: no test or script reads a binary from a crate's own target/ (the workspace builds to one root target/)" {
  run old_paths "$ROOT"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "#4915 NEGATIVE: a test naming a crate's own target/debug is caught by name" {
  local t="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$t/platform/tests" "$t/platform/scripts" "$t/skills" "$t/proving/scripts" "$t/proving/config"
  printf 'BIN="$ROOT/platform/services/borg-prove/target/debug/borg-prove"\n' > "$t/platform/tests/old.bats"
  printf 'D="${CARGO_TARGET_DIR:-$ROOT/platform/services/chorus-hooks/target}/debug/x"\n' > "$t/platform/tests/old2.bats"
  printf 'BIN="$ROOT/target/debug/borg-prove"\n' > "$t/platform/tests/new.bats"
  run old_paths "$t"
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 2 ] || { echo "$output"; return 1; }
  [[ "$output" == *"platform/tests/old.bats:1"* && "$output" == *"platform/tests/old2.bats:1"* ]] || { echo "$output"; return 1; }
}

fixed_root_paths() {  # repo root → each "file:line" reading a binary at a fixed target/<profile>/ path
  (cd "$1" && grep -rnE '(ROOT|REPO|CHORUS_ROOT|\.\./\.\.)\}?/target/(debug|release)/[a-z]' platform/tests platform/scripts skills proving/scripts 2>/dev/null) \
    | grep -vE "^(platform/tests/4915-workspace-target-paths.bats|platform/scripts/lib/workspace-bin.sh):" || true
}

@test "#4915: a binary is found through workspace_bin, never at a fixed target/<profile>/ (pipeline builds land in target/<account>)" {
  run fixed_root_paths "$ROOT"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "#4915 NEGATIVE: a fixed \$ROOT/target/release path is caught; workspace_bin answers the pipeline's place first" {
  local t="$BATS_TEST_TMPDIR/tree2"
  mkdir -p "$t/platform/tests" "$t/platform/scripts" "$t/skills" "$t/proving/scripts"
  printf 'SHIM="$CHORUS_ROOT/target/release/chorus-hook-shim"\n' > "$t/platform/tests/fixed.sh"
  run fixed_root_paths "$t"
  [[ "$output" == *"platform/tests/fixed.sh:1"* ]] || { echo "$output"; return 1; }
  # the lookup: the account's target wins over target/, and a missing binary names the account's place
  local r="$BATS_TEST_TMPDIR/repo"; mkdir -p "$r/target/$(id -un)/release" "$r/target/release"
  touch "$r/target/$(id -un)/release/x" "$r/target/release/x"; chmod +x "$r/target/$(id -un)/release/x" "$r/target/release/x"
  . "$ROOT/platform/scripts/lib/workspace-bin.sh"
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" release x)" = "$r/target/$(id -un)/release/x" ]
  [ "$(CARGO_TARGET_DIR= workspace_bin "$r" release nope)" = "$r/target/$(id -un)/release/nope" ]
}

# the two scripts this card moved to the lookup, and the hooks LaunchAgent copy Silas asked for
@test "#4915: the scripts and unregistered shell tests this slice moved find their binary through workspace_bin" {
  # and the three unregistered shell tests this slice moved (no registered test runs them; this names them)
  for f in platform/scripts/athena-model-gate.sh proving/scripts/post-cargo-build.sh \
           platform/tests/athena-model-e2e.sh platform/tests/context-cache-weekly.sh platform/tests/inject-regression.sh; do
    grep -q 'workspace-bin.sh' "$ROOT/$f" || { echo "$f does not source the lookup"; return 1; }
    grep -q 'workspace_bin "' "$ROOT/$f" || { echo "$f does not ask workspace_bin"; return 1; }
  done
}

@test "#4915: the hooks LaunchAgent copy runs the installed binary, like live and launchagents-canonical" {
  local p="$ROOT/proving/config/launchagents/com.chorus.hooks.plist"
  grep -qE '<string>/Users/[^/]+/\.chorus/bin/chorus-hooks</string>' "$p" || return 1
  ! grep -q '/target/' "$p" || { grep -n '/target/' "$p"; return 1; }
}
