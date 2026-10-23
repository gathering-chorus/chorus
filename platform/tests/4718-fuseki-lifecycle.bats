#!/usr/bin/env bats
# @test-type: unit — fuseki-ctl with a fake launchctl and a fake Fuseki ping; app-state.sh read as source text. Nothing real starts or stops.
# @domain: persistence — Fuseki is the store; one lifecycle surface for it (#4718)
#
# #4718 — `app-state.sh restart fuseki` restarted the gathering app, not Fuseki
# (pid on :3030 unchanged, 10-22), and `app-state.sh stop` stopped the store
# both products share. Fuseki's lifecycle is fuseki-ctl's alone.

ROOT="$BATS_TEST_DIRNAME/../.."

setup() {
  T="$(mktemp -d)"; mkdir -p "$T/bin"
  printf '#!/bin/sh\necho "$*" >> "%s/calls"\ncase "$1" in print) [ -e "%s/loaded" ] || exit 1; echo "state = running";; esac\nexit 0\n' "$T" "$T" > "$T/bin/launchctl"
  printf '#!/bin/sh\nexit 0\n' > "$T/bin/curl"
  chmod +x "$T/bin/launchctl" "$T/bin/curl"
}
teardown() { rm -rf "$T"; }

ctl() { run env PATH="$T/bin:$PATH" FUSEKI_LAUNCHCTL="$T/bin/launchctl" FUSEKI_PLIST="$T/f.plist" bash "$ROOT/platform/scripts/fuseki-ctl" "$@"; }

@test "restart kickstarts Fuseki's own job (-k) and waits for it to answer" {
  touch "$T/loaded"
  ctl restart
  [ "$status" -eq 0 ] || return 1
  grep -qE "^kickstart -k gui/[0-9]+/com.gathering.fuseki$" "$T/calls" || { cat "$T/calls"; false; }
  [[ "$output" == *"back after"* ]] || return 1
}

@test "stop boots the job out (KeepAlive would revive a killed one); start bootstraps it back" {
  ctl stop
  grep -qE "^bootout gui/[0-9]+/com.gathering.fuseki$" "$T/calls" || return 1
  ctl start
  grep -qE "^bootstrap gui/[0-9]+ $T/f.plist$" "$T/calls" || { cat "$T/calls"; false; }
}

@test "app-state.sh neither starts nor stops Fuseki" {
  ! grep -nE 'service_(start|stop) "\$FUSEKI_LABEL"' "$ROOT/platform/scripts/app-state.sh" || return 1
}

@test "negative proof: a planted Fuseki stop in app-state.sh is caught" {
  cp "$ROOT/platform/scripts/app-state.sh" "$T/app-state.sh"
  printf '\nservice_stop "$FUSEKI_LABEL"\n' >> "$T/app-state.sh"
  grep -qE 'service_(start|stop) "\$FUSEKI_LABEL"' "$T/app-state.sh"
}
