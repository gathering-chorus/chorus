#!/usr/bin/env bats
# @test-type: unit — chorus-bin-install into a temp slot under the role umask; no live slot is touched
# @domain: security
# @card: #4853 · owner: silas
# What Jeff sees: no role account can get its own code run as his account. The
# runner and the verb wrappers refuse a slot that group/other can write; this
# proves the installers never make one, even under a role's umask 002 (how the
# live silas-bin and borg-queen-bin became group-writable).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

setup() {
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/home/.chorus"
  printf '#!/bin/sh\necho hi\n' > "$T/src"; chmod 0775 "$T/src"
}

mode() { stat -f '%Lp' "$1"; }

@test "a werk-slot install under umask 002 leaves the slot dir and binary 0755" {
  run bash -c 'umask 002; HOME="$1/home" CHORUS_BIN_SPINE_LOG="$1/spine.log" CHORUS_BIN_SKIP_SMOKE=1 CHORUS_ROLE=wren WERK_WREN_BIN="$1/wren-bin" "$2" --target werk "$1/src" probe-4853' _ "$T" "$ROOT/platform/scripts/chorus-bin-install"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(mode "$T/wren-bin")" = "755" ] || { echo "dir $(mode "$T/wren-bin")"; return 1; }
  [ "$(mode "$T/wren-bin/probe-4853")" = "755" ] || { echo "file $(mode "$T/wren-bin/probe-4853")"; return 1; }
}

@test "negative proof: the same copy without the fix would be 775 (the umask really is 002 here)" {
  run bash -c 'umask 002; mkdir "$1/plain"; cp -p "$1/src" "$1/plain/x"; stat -f %Lp "$1/plain" "$1/plain/x"' _ "$T"
  [[ "$output" == *"775"*"775"* ]] || { echo "$output"; return 1; }
}
