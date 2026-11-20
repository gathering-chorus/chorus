#!/usr/bin/env bats
# @test-type: unit — stand-in git, dagu and athena-deploy record their calls; no live scheduler
# @domain: pipelines
# @card: 4995 · owner: kade
# What the card hit (Borg Queen 18:55): #4995 changes platform/services/shared/crate_dirs.rs,
# which werk-test compiles in with include!. pipeline-start refreshed every slot verb the
# card "does not change" from the installed copy, so the card's own werk-test was
# replaced by the old one, which refused the card's new crate home at scope.
# Now a changed shared file counts as a change to every crate that includes it.

setup() {
  export MOB_OPEN_BIN=/usr/bin/true PAIR_RERUN_BIN=/usr/bin/true
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks" CHORUS_BIN="$BATS_TEST_TMPDIR/installed"
  W="$CHORUS_WERK_BASE/kade-4995"
  SLOT="$CHORUS_WERK_BASE/kade-bin"
  mkdir -p "$CHORUS_HOME" "$W/platform/pipelines" "$SLOT" "$CHORUS_BIN" "$BATS_TEST_TMPDIR/bin" \
           "$W/platform/services/shared" "$W/platform/services/werk-test/src" "$W/platform/services/werk-pull/src"
  touch "$W/platform/pipelines/cicd.yaml" "$W/platform/services/shared/crate_dirs.rs"
  printf '[package]\nname = "werk-test"\n' > "$W/platform/services/werk-test/Cargo.toml"
  printf 'include!("../../shared/crate_dirs.rs");\n' > "$W/platform/services/werk-test/src/lib.rs"
  printf '[package]\nname = "werk-pull"\n' > "$W/platform/services/werk-pull/Cargo.toml"
  printf 'fn main() {}\n' > "$W/platform/services/werk-pull/src/main.rs"
  # slot copies are the card's own builds; the installed ones are older
  for v in werk-test werk-pull; do printf 'CARD-BUILD' > "$SLOT/$v"; printf 'INSTALLED' > "$CHORUS_BIN/$v"; done
  export DAGU="$BATS_TEST_TMPDIR/dagu" ATHENA_DEPLOY="$BATS_TEST_TMPDIR/athena-deploy"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"
  printf '#!/bin/sh\nexit 0\n' > "$ATHENA_DEPLOY"
  printf '#!/bin/sh\necho platform/services/shared/crate_dirs.rs\n' > "$BATS_TEST_TMPDIR/bin/git"
  chmod +x "$DAGU" "$ATHENA_DEPLOY" "$BATS_TEST_TMPDIR/bin/git"
  export PATH="$BATS_TEST_TMPDIR/bin:$PATH"
}

@test "a changed shared file keeps the card's own build of every verb that includes it" {
  run "$ROOT/platform/scripts/pipeline-start" cicd 4995 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$SLOT/werk-test")" = CARD-BUILD ] || { echo "werk-test was replaced: $output"; return 1; }
  # a verb that does not include the shared file is still refreshed
  [ "$(cat "$SLOT/werk-pull")" = INSTALLED ] || { echo "werk-pull was not refreshed: $output"; return 1; }
}

@test "negative proof: when no crate includes the shared file, every verb is refreshed" {
  printf 'fn nothing() {}\n' > "$W/platform/services/werk-test/src/lib.rs"
  run "$ROOT/platform/scripts/pipeline-start" cicd 4995 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$SLOT/werk-test")" = INSTALLED ] || { echo "$output"; return 1; }
}
