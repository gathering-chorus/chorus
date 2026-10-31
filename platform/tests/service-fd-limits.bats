#!/usr/bin/env bats
# @test-type: integration — the LaunchAgent sources and a real pulse-api process under a lowered file limit
# @domain: services
#
# #4814. On 2026-10-30 15:32 a burst of 40 prompts crashed pulse-api's listener with
# "Too many open files": launchd gave it the soft default of 256 files, and the accept
# thread panicked on the first EMFILE while the process stayed alive. These checks
# fail on a missing plist, a missing binary or a missing key, never skip (#3734).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CANON="$ROOT/platform/scripts/launchagents-canonical"
  MIN_FILES=4096
}

# The soft NumberOfFiles a plist sets, or 0 when it sets none.
soft_files() {
  /usr/libexec/PlistBuddy -c 'Print :SoftResourceLimits:NumberOfFiles' "$1" 2>/dev/null || echo 0
}

@test "AC1: pulse-api and hooks LaunchAgents set SoftResourceLimits NumberOfFiles >= 4096" {
  for label in com.chorus.pulse-api com.chorus.hooks; do
    plist="$CANON/$label.plist"
    [ -f "$plist" ] || { echo "missing canonical plist: $plist"; return 1; }
    n=$(soft_files "$plist")
    echo "$label soft NumberOfFiles=$n"
    [ "$n" -ge "$MIN_FILES" ] || return 1
  done
}

@test "AC1 negative proof: a plist with no SoftResourceLimits reads as 0 and fails the floor" {
  bare="$BATS_TEST_TMPDIR/bare.plist"
  /usr/libexec/PlistBuddy -c 'Add :Label string com.chorus.fixture' "$bare" >/dev/null
  n=$(soft_files "$bare")
  [ "$n" -eq 0 ]
  [ "$n" -lt "$MIN_FILES" ] || return 1
}

# A pulse-api on a scratch port with no reachable store, with the soft file limit at
# 64, takes a 100-connection burst and must exit rather than hang.
burst_then_exit() {
  bin="$1"
  [ -x "$bin" ] || { echo "no pulse-api binary at $bin"; return 1; }
  port=$((20000 + RANDOM % 20000))
  ( ulimit -n 64
    # sparql at a dead endpoint: /health never touches the store, and an embedded
    # store needs 96+ files just to open, more than this fixture allows
    PULSE_SERVER_BIND="127.0.0.1:$port" PULSE_STORE_KIND=sparql \
      PULSE_STORE_ENDPOINT="http://127.0.0.1:9/none" PULSE_SETTINGS=/dev/null \
      exec "$bin" ) >"$BATS_TEST_TMPDIR/pulse.log" 2>&1 &
  pid=$!
  before=000
  for _ in $(seq 1 50); do
    before=$(curl -s -m 1 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/health" || true)
    [ "$before" = "200" ] && break
    sleep 0.1
  done
  # Two states, two messages: a server that never came up is not one the burst killed.
  if [ "$before" != "200" ]; then
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true
    echo "pulse-api never answered before the burst (/health -> $before)"; head -20 "$BATS_TEST_TMPDIR/pulse.log"
    return 2
  fi
  # Quick requests close before the next opens, so they never reach 64 at once (the
  # first fixture stayed green on the broken binary). Hold 100 connections open
  # together for 2 s, which is what a burst of slow prompts does.
  python3 -I -c '
import socket, sys, time
held = []
for _ in range(100):
    try:
        held.append(socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=2))
    except OSError:
        pass
time.sleep(2)
for s in held: s.close()
print("held", len(held))
' "$port"
  # The broken state is a live process that never answers again; the fixed one exits
  # (code 70, said on stderr) so launchd's KeepAlive starts a fresh api. No launchd
  # here, so the test asserts the exit; the restart is proven live after the land.
  for _ in $(seq 1 50); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    code=$(curl -s -m 3 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/health" || true)
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true
    echo "still alive 5 s after the burst, /health -> $code (the hung listener)"
    grep -i "too many open files" "$BATS_TEST_TMPDIR/pulse.log" | head -2 || true
    return 1
  fi
  rc=0; wait "$pid" || rc=$?
  echo "exited $rc"
  [ "$rc" -eq 70 ] || return 1
  grep -q "a thread panicked — exiting" "$BATS_TEST_TMPDIR/pulse.log"
}

# The werk's own build: the per-account folder a role builds into (#4772), else the
# pipeline's target/release.
pulse_bin() {
  local t="$ROOT/directing/products/pulse/api/target"
  if [ -n "${PULSE_API_BIN:-}" ]; then echo "$PULSE_API_BIN"
  elif [ -x "$t/$(id -un)/release/pulse-api" ]; then echo "$t/$(id -un)/release/pulse-api"
  else echo "$t/release/pulse-api"; fi
}

@test "AC2+AC3: with the file limit at 64, a 100-connection burst ends pulse-api (exit 70) instead of hanging it" {
  burst_then_exit "$(pulse_bin)"
}

@test "AC3 negative proof: a server that answers /health and never exits is caught as hung" {
  fake="$BATS_TEST_TMPDIR/fake-pulse-api"
  cat >"$fake" <<'EOF'
#!/usr/bin/env python3
# answers /health 200 forever and never exits — the hung-listener state, minus the panic
import os, http.server
host, port = os.environ["PULSE_SERVER_BIND"].rsplit(":", 1)
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b"ok")
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer((host, int(port)), H).serve_forever()
EOF
  chmod +x "$fake"
  run burst_then_exit "$fake"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"still alive 5 s after the burst"* ]] || return 1
}

@test "AC2 restart: the pulse-api LaunchAgent keeps the api alive, so an exit is a restart" {
  [ "$(/usr/libexec/PlistBuddy -c 'Print :KeepAlive' "$CANON/com.chorus.pulse-api.plist")" = "true" ]
}
