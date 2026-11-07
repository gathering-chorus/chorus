#!/usr/bin/env bats
# @test-type: fitness — static scan of four scripts, no live service
# @domain: builds
# @card: #4915 · owner: kade
# Silas 11-06: prod runs ~/.chorus/bin; build-signed now builds per account, so a
# target/release fallback in these four is no account's install. They say so and stop
# instead. (resolve-shim/shim-path.ts and werk's status read canonical's target/release,
# which the land itself refreshes — werk-deploy copies built binaries there.)
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
FILES="platform/scripts/agent-state.sh platform/scripts/shim-wrapper.sh platform/scripts/spine-tick-poller platform/scripts/pair-enforce"

readers() { local f; for f in "$@"; do grep -v '^[[:space:]]*#' "$f" | grep -l 'target/release' >/dev/null && echo "$f"; done; true; }

@test "the four retired readers no longer run a target/release binary" {
  local f hits=""
  for f in $FILES; do [ -f "$ROOT/$f" ] || { echo "missing $f"; return 1; }; done
  hits="$(cd "$ROOT" && readers $FILES)"
  [ -z "$hits" ] || { echo "still reading target/release: $hits"; return 1; }
}

@test "negative proof: a reader with a target/release fallback is caught" {
  printf 'X="${CHORUS_ROOT}/platform/services/chorus-inject/target/release/chorus-inject"\n' > "$BATS_TEST_TMPDIR/bad"
  printf '# a comment naming target/release is prose\nX=ok\n' > "$BATS_TEST_TMPDIR/good"
  [ "$(readers "$BATS_TEST_TMPDIR/bad" "$BATS_TEST_TMPDIR/good")" = "$BATS_TEST_TMPDIR/bad" ] || return 1
}
