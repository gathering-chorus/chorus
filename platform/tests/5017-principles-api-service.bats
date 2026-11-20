#!/usr/bin/env bats
# @test-type: integration — starts the card's principles-api build on a free port; reads no live store
# @domain: principles
# @card: 5017 · owner: kade
# What Jeff sees: /owl/v1/principles/principles answers from principles-api, the
# principles domain's own generated API, which runs as a LaunchAgent like every service.
#
# AC1: a launchagents-canonical plist runs the generated binary through its launch
# wrapper with its settings row (store.kind sparql on the pod, bind 127.0.0.1:3393);
# the card's build, started through that wrapper, answers /health and creates no
# embedded store. NEGATIVE: the same start with store.kind=embedded does create one,
# and the check below goes red on it.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CRATE="$REPO/shaping/domains/principles/api"
PLIST="$REPO/platform/scripts/launchagents-canonical/com.chorus.principles-api.plist"
LAUNCH="$CRATE/principles-api-launch.sh"

# The card's build: PRINCIPLES_API_BIN, else the workspace target, else the crate's own.
# Built once if missing; a failed build fails the test, never skips it.
card_bin() {
  local c
  for c in "${PRINCIPLES_API_BIN:-}" "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/principles-api}" \
           "$REPO/target/release/principles-api" "$CRATE/target/release/principles-api"; do
    [ -n "$c" ] && [ -x "$c" ] && { echo "$c"; return 0; }
  done
  (cd "$CRATE" && cargo build --release --quiet) >&2 || return 1
  for c in "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/principles-api}" \
           "$REPO/target/release/principles-api" "$CRATE/target/release/principles-api"; do
    [ -n "$c" ] && [ -x "$c" ] && { echo "$c"; return 0; }
  done
  return 1
}

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

# Starts the launch wrapper with the card's build; KIND overrides store.kind.
# Prints the pid. The store path is a fresh, absent folder under the test's tmp.
start() {
  local kind="$1" port="$2" store="$3" bin
  bin="$(card_bin)" || { echo "principles-api not built" >&2; return 1; }
  env PRINCIPLES_API_BIN="$bin" PRINCIPLES_SERVER_BIND="127.0.0.1:$port" \
      PRINCIPLES_STORE_PATH="$store" PRINCIPLES_EVENTS_COMMAND="" \
      ${kind:+PRINCIPLES_STORE_KIND=$kind} FUSEKI_WRITE_ENV=/nonexistent \
      bash "$LAUNCH" >"$BATS_TEST_TMPDIR/out.log" 2>&1 &
  echo $!
}

# 0 when the service answers /health as principles within 30 s
healthy() {
  local port="$1" i
  for i in $(seq 1 60); do
    curl -sf --max-time 2 "http://127.0.0.1:$port/health" | grep -q '"product":"principles"\|"product": "principles"' && return 0
    sleep 0.5
  done
  return 1
}

# The AC's check: healthy AND no embedded store made
service_ok() { healthy "$1" && [ ! -e "$2" ]; }

teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

@test "AC1: the plist runs the launch wrapper under service-run, KeepAlive, no credential in it" {
  plutil -lint "$PLIST"
  run python3 - "$PLIST" <<'PY'
import plistlib, sys
p = plistlib.load(open(sys.argv[1], "rb"))
assert p["Label"] == "com.chorus.principles-api", p["Label"]
a = p["ProgramArguments"]
assert a[0].endswith("/platform/scripts/service-run") and a[1] == "com.chorus.principles-api" and a[2] == "daemon", a
assert a[3].endswith("/shaping/domains/principles/api/principles-api-launch.sh"), a
assert p.get("KeepAlive") is True and p.get("RunAtLoad") is True
env = p.get("EnvironmentVariables", {})
assert not any("PASSWORD" in k or "TOKEN" in k for k in env), env
print("ok")
PY
  [ "$status" -eq 0 ] && [ "$output" = "ok" ] || { echo "$output"; false; }
  # the wrapper passes the credential by name and execs the generated binary
  grep -q 'exec "${PRINCIPLES_API_BIN:-$HOME/.chorus/bin/principles-api}"' "$LAUNCH"
  ! grep -qE 'FUSEKI_ADMIN_PASSWORD=[A-Za-z0-9]' "$LAUNCH" || return 1
}

@test "AC1: the settings row says store.kind sparql on the pod, bind 127.0.0.1:3393" {
  grep -q '("store.kind", "sparql")' "$CRATE/src/model.rs"
  grep -q '("store.endpoint", "http://localhost:3030/pods")' "$CRATE/src/model.rs"
  grep -q '("server.bind", "127.0.0.1:3393")' "$CRATE/src/model.rs"
}

@test "AC1: the card's build, started through the wrapper, answers /health and creates no embedded store" {
  local port store
  port="$(free_port)"; store="$BATS_TEST_TMPDIR/embedded-store"
  PID="$(start "" "$port" "$store")"
  service_ok "$port" "$store" || { cat "$BATS_TEST_TMPDIR/out.log"; ls -la "$store" 2>&1; false; }
}

@test "AC1 NEGATIVE: started with store.kind=embedded it makes a store, and the check goes red" {
  local port store
  port="$(free_port)"; store="$BATS_TEST_TMPDIR/embedded-store"
  PID="$(start embedded "$port" "$store")"
  healthy "$port" || { cat "$BATS_TEST_TMPDIR/out.log"; false; }
  [ -e "$store" ]
  ! service_ok "$port" "$store" || return 1
}
