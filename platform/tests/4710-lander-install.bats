#!/usr/bin/env bats
# @test-type: integration — the lander's install script against a fake root (ROOT prefix, stub launchctl/chown/lsof); nothing on this Mac is installed or booted
# @domain: security
# @card: #4710 · owner: wren
# What Jeff sees: one sudo command installs the lander as its own account, retires the old go
# page, and proves one listener on :3490 — or refuses and says why (Silas 18:30).
load test_helper
SCRIPT="$CHORUS_ROOT/platform/scripts/lander-install"

setup() {
  R="$BATS_TEST_TMPDIR/root"; mkdir -p "$R/home/lander" "$R/jeff/Library/LaunchAgents" "$BATS_TEST_TMPDIR/bin"
  echo old > "$R/jeff/Library/LaunchAgents/com.chorus.go.plist"
  SRC="$BATS_TEST_TMPDIR/chorus-go"; printf '#!/bin/sh\necho lander\n' > "$SRC"; chmod 755 "$SRC"
  LOG="$BATS_TEST_TMPDIR/launchctl.log"
  printf '#!/bin/sh\necho "$@" >> %s\n' "$LOG" > "$BATS_TEST_TMPDIR/bin/launchctl"; chmod 755 "$BATS_TEST_TMPDIR/bin/launchctl"
  # a stand-in `ps -o user= -p <pid>`: prints the account named by its file name, ignores the pid
  for u in chorus-lander jeffbridwell; do printf '#!/bin/sh\necho %s\n' "$u" > "$BATS_TEST_TMPDIR/bin/owner-$u"; chmod 755 "$BATS_TEST_TMPDIR/bin/owner-$u"; done
  export ROOT="$R" LANDER_HOME=/home/lander LANDER_UID="$(id -u)" LANDER_GID="$(id -g)" \
         JEFF_HOME=/jeff JEFF_UID=501 LAUNCHCTL="$BATS_TEST_TMPDIR/bin/launchctl" CHOWN=true LISTENERS="echo 4242" OWNER_OF="$BATS_TEST_TMPDIR/bin/owner-chorus-lander"
}

@test "a fresh install: lander-owned bin and binary, the daemon plist, the old go page retired, one listener" {
  run "$SCRIPT" "$SRC"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"sha256 $(shasum -a 256 "$SRC" | awk '{print $1}')"* ]] || return 1
  [ "$(stat -f %Lp "$R/home/lander/bin")" = 755 ] && [ "$(stat -f %Lp "$R/home/lander/bin/chorus-go")" = 755 ] || return 1
  P="$R/Library/LaunchDaemons/com.chorus.lander.plist"
  [ "$(stat -f %Lp "$P")" = 644 ]
  plutil -lint "$P" >/dev/null
  [ "$(plutil -extract UserName raw "$P")" = chorus-lander ]
  [ "$(plutil -extract EnvironmentVariables.PATH raw "$P")" = /usr/bin:/bin ]
  [ "$(plutil -extract ProgramArguments.0 raw "$P")" = /home/lander/bin/chorus-go ]
  [ ! -e "$R/jeff/Library/LaunchAgents/com.chorus.go.plist" ]
  grep -q 'bootout gui/501/com.chorus.go' "$LOG" && grep -q "bootstrap system $P" "$LOG" || return 1
}

@test "a second run with the same binary changes nothing it need not" {
  "$SCRIPT" "$SRC" >/dev/null
  : > "$LOG"
  run "$SCRIPT" "$SRC"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"already installed"* ]] || return 1
  ! grep -q bootstrap "$LOG" || { cat "$LOG"; return 1; }
}

# NEGATIVE PROOF (Silas 18:30): a bin dir another account owns could hold a swapped binary
@test "negative: a lander bin dir owned by another account makes the install refuse" {
  mkdir -p "$R/home/lander/bin"
  run env LANDER_UID=$(( $(id -u) + 1 )) "$SCRIPT" "$SRC"
  [ "$status" -eq 1 ] && [[ "$output" == *"is owned by uid $(id -u), not chorus-lander"* ]] || { echo "$output"; return 1; }
  [ ! -e "$R/Library/LaunchDaemons/com.chorus.lander.plist" ]
}

@test "negative: two listeners on :3490 (the old go page still up) is refused" {
  run env LISTENERS="printf 4242\n4343\n" "$SCRIPT" "$SRC"
  [ "$status" -eq 1 ] && [[ "$output" == *"expected one listener on :3490, found 2"* ]] || { echo "$output"; return 1; }
}

@test "negative: no lander home, no binary: refused before anything is written" {
  run env LANDER_HOME=/nope "$SCRIPT" "$SRC"
  [ "$status" -eq 1 ] && [[ "$output" == *"has no home"* ]] || return 1
  run "$SCRIPT" /nope/chorus-go
  [ "$status" -eq 1 ] && [[ "$output" == *"give the path"* ]] || return 1
  [ ! -e "$R/Library/LaunchDaemons/com.chorus.lander.plist" ]
}

@test "negative: one listener on :3490 that is not the lander's process (jeffbridwell's old go page) is refused" {
  run env OWNER_OF="$BATS_TEST_TMPDIR/bin/owner-jeffbridwell" "$SCRIPT" "$SRC"
  [ "$status" -eq 1 ] && [[ "$output" == *"runs as 'jeffbridwell', not chorus-lander"* ]] || { echo "$output"; return 1; }
}
