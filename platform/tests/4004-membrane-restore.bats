#!/usr/bin/env bats
# @test-type: unit — drives the restore against a stubbed launchd; nothing real is booted
# @domain: tests — the product domain this suite guards (#4334)
#
# #4004 reopen — the attended run at 2026-10-12 08:07 proved the membrane and
# left com.chorus.session-watcher down ("restarted 18/19"). A bootout returns
# before launchd has unloaded the agent, so three bootstraps one second apart
# can all land while the label is still there. hooks or api left down that way
# is a team lockout. These run the whole script against a fake launchctl in
# which one agent takes several calls to unload.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/test-product-membrane.sh"

setup() {
  STUB="$BATS_TEST_TMPDIR/bin"
  STATE="$BATS_TEST_TMPDIR/launchd"
  mkdir -p "$STUB" "$STATE"
  for l in com.chorus.api com.chorus.session-watcher com.chorus.hooks; do echo loaded > "$STATE/$l"; done
  # launchctl: list | print gui/U/L | bootout gui/U/L | bootstrap gui/U PLIST
  # An agent booted out stays in the domain for $SLOW_TICKS more calls that
  # name it (only SLOW_LABEL is slow); bootstrap of a label still there fails.
  cat > "$STUB/launchctl" <<'EOF'
#!/usr/bin/env bash
S="$LAUNCHD_STATE"
tick() { # $1 label — an unloading agent counts down on every call naming it
  local f="$S/$1"; [ -f "$f" ] || return 0
  local st; st=$(cat "$f")
  case "$st" in unloading:*) n=${st#unloading:}; if [ "$n" -le 1 ]; then rm -f "$f"; else echo "unloading:$((n-1))" > "$f"; fi ;; esac
}
case "$1" in
  list)
    for f in "$S"/*; do [ -f "$f" ] || continue; [ "$(cat "$f")" = loaded ] && echo "100	0	$(basename "$f")"; done; exit 0 ;;
  print)
    l=${2##*/}; tick "$l"; [ -f "$S/$l" ] ;;
  bootout)
    l=${2##*/}; [ -f "$S/$l" ] || exit 3
    if [ "$l" = "$SLOW_LABEL" ]; then echo "unloading:$SLOW_TICKS" > "$S/$l"; else rm -f "$S/$l"; fi ;;
  bootstrap)
    l=$(basename "$3" .plist); tick "$l"
    if [ -f "$S/$l" ]; then echo "Bootstrap failed: 5: Input/output error" >&2; exit 5; fi
    echo loaded > "$S/$l" ;;
esac
EOF
  printf '#!/bin/sh\nprintf 200\n' > "$STUB/curl"
  printf '#!/bin/sh\nexit 0\n' > "$STUB/sleep"
  chmod +x "$STUB"/*
}

run_membrane() {
  run env -u WERK_TEST_UNIT_TIMEOUT -u WERK_TEST_NIGHTLY PATH="$STUB:$PATH" LAUNCHD_STATE="$STATE" \
    SLOW_LABEL=com.chorus.session-watcher SLOW_TICKS=6 MEMBRANE_ALLOW_UNDER_AGENT=1 \
    MEMBRANE_LEDGER="$BATS_TEST_TMPDIR/ledger" MEMBRANE_RESULTS="$BATS_TEST_TMPDIR/results" "$@" bash "$SCRIPT"
}

@test "an agent slow to unload is waited for, and every stopped agent comes back" {
  run_membrane
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"MEMBRANE OK"* ]] || return 1
  [[ "$output" == *"restarted: 3/3"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"STILL DOWN"* ]] || return 1
  for l in com.chorus.api com.chorus.session-watcher com.chorus.hooks; do [ "$(cat "$STATE/$l")" = loaded ] || return 1; done
}

@test "NEGATIVE PROOF: with no wait the slow agent stays down — named, and the run fails" {
  run_membrane MEMBRANE_UNLOAD_WAIT=0
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"STILL DOWN"*"com.chorus.session-watcher"* ]] || { echo "$output"; return 1; }
  [ ! -f "$STATE/com.chorus.session-watcher" ] || [ "$(cat "$STATE/com.chorus.session-watcher")" != loaded ] || return 1
}

@test "an agent that never unloads is reported down by name, not waited on forever" {
  run_membrane SLOW_TICKS=1000 MEMBRANE_UNLOAD_WAIT=5
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"STILL DOWN"*"com.chorus.session-watcher"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"restarted: 2/3"* ]] || { echo "$output"; return 1; }
}
