#!/usr/bin/env bats
# @test-type: unit — the tracked plist, clearing-room-env on a temp secret, and land-4984-rows
# against a fake launchctl, a fake Clearing and a fake Loki; nothing live is touched
# @domain: clearing
# @card: #4984
#
# AC3: the current Clearing (v1) starts from one tracked launcher in launchagents-canonical that
# passes the relay Host and BUZZ_AUTH_RELAY_URL; the untracked ~/.chorus/bin/clearing-launch.sh
# is retired in the same land, and only after the room is proven to reach the relay.
#
# Covers: platform/scripts/launchagents-canonical/com.chorus.clearing.plist
# Covers: platform/scripts/clearing-room-env
# Covers: platform/scripts/land-4984-rows

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  PLIST="$ROOT/platform/scripts/launchagents-canonical/com.chorus.clearing.plist"
  LAND="$ROOT/platform/scripts/land-4984-rows"
  T="$BATS_TEST_TMPDIR"
}

teardown() {
  [ -f "$T/fake.pid" ] && kill "$(cat "$T/fake.pid")" 2>/dev/null || true
}

# the launcher's verdict on a plist: '' and 0 when it passes, else the reason and 1
launcher_ok() {
  python3 - "$1" <<'PY'
import plistlib, sys
p = plistlib.load(open(sys.argv[1], "rb"))
env, args = p.get("EnvironmentVariables", {}), p.get("ProgramArguments", [])
why = []
if p.get("Label") != "com.chorus.clearing": why.append("label")
if not any(a.endswith("platform/scripts/clearing-room-env") for a in args): why.append("not started through clearing-room-env")
if not any(a.endswith("directing/clearing/dist/server.js") for a in args): why.append("not the v1 Clearing")
if any("clearing-launch.sh" in a for a in args): why.append("still runs the untracked launcher")
if env.get("BUZZ_RELAY_HOST_HEADER") != "buzz.lightlifeurbangardens.com": why.append("no relay Host")
if not env.get("BUZZ_AUTH_RELAY_URL", "").startswith("wss://"): why.append("no public address for the sign-in")
if any("SECRET" in k for k in env): why.append("a secret in the plist")
print("; ".join(why)); sys.exit(1 if why else 0)
PY
}

@test "the tracked plist starts the v1 Clearing through clearing-room-env with the Host and the public address" {
  run launcher_ok "$PLIST"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a launcher without the Host fails the check" {
  sed '/BUZZ_RELAY_HOST_HEADER/d' "$PLIST" > "$T/nohost.plist"
  if cmp -s "$PLIST" "$T/nohost.plist"; then echo "mutation did not apply"; return 1; fi
  run launcher_ok "$T/nohost.plist"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'no relay Host'
}

@test "clearing-room-env turns the room on from the 600 secret file and passes the Host through untouched" {
  printf 's3cret' > "$T/secret"; chmod 600 "$T/secret"
  run env BUZZ_ROOM_SECRET_FILE="$T/secret" BUZZ_RELAY_HOST_HEADER=buzz.example \
    "$ROOT/platform/scripts/clearing-room-env" sh -c 'echo "$BUZZ_ROOM_ENABLED|$BUZZ_RELAY_HOST_HEADER|${#BUZZ_ROOM_SECRET}"'
  [ "$status" -eq 0 ]
  [ "$output" = "1|buzz.example|6" ]
}

# --- the land, against fakes ------------------------------------------------------------

# a fake Clearing (/health 200) and a fake Loki serving $T/room.json as the bridge's events
start_fakes() {
  python3 - "$T" >"$T/fake.port" 2>/dev/null <<'PY' &
import http.server, json, socket, sys, time
T = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path.startswith("/health"):
            body = b"ok"
        else:
            lines = json.load(open(f"{T}/room.json"))
            now = time.time_ns()
            body = json.dumps({"data": {"result": [{"stream": {}, "values": [[str(now + i), json.dumps(e)] for i, e in enumerate(lines)]}]}}).encode()
        self.send_response(200); self.end_headers(); self.wfile.write(body)
s = http.server.HTTPServer(("127.0.0.1", 0), H)
print(s.server_address[1], flush=True)
s.serve_forever()
PY
  echo $! > "$T/fake.pid"
  for _ in $(seq 50); do [ -s "$T/fake.port" ] && break; sleep 0.1; done
  PORT=$(cat "$T/fake.port")
}

fake_land() {
  mkdir -p "$T/agents" "$T/bin"
  printf '<plist>old</plist>' > "$T/agents/com.chorus.clearing.plist"
  printf '#!/bin/bash\n' > "$T/bin/clearing-launch.sh"
  cat > "$T/launchctl" <<'SH'
#!/bin/bash
echo "$*" >> "$(dirname "$0")/launchctl.log"
[ "$1" = print ] && exit 1
exit 0
SH
  chmod +x "$T/launchctl"
  start_fakes
  CHORUS_HOME="$ROOT" LAND_AS_UID="${LAND_AS_UID:-$(id -u)}" LAUNCH_AGENTS_DIR="$T/agents" LAUNCHCTL="$T/launchctl" \
    OLD_LAUNCHER="$T/bin/clearing-launch.sh" CLEARING_URL="http://127.0.0.1:$PORT" LOKI_URL="http://127.0.0.1:$PORT" \
    CLEARING_READY_WAIT=3 ROOM_WAIT=3 LAUNCHD_RELEASE_WAIT=2 run python3 "$LAND" --apply
}

@test "the land installs the tracked plist and retires the old launcher once the room signs in" {
  echo '[{"event":"buzz.room.connected"},{"event":"buzz.room.authenticated","accepted":true}]' > "$T/room.json"
  fake_land
  echo "$output"
  [ "$status" -eq 0 ]
  cmp -s "$PLIST" "$T/agents/com.chorus.clearing.plist"
  [ ! -e "$T/bin/clearing-launch.sh" ]
  grep -q '^bootstrap ' "$T/launchctl.log"
}

@test "NEGATIVE PROOF: a 404 at the dial puts the old plist back and keeps the old launcher" {
  echo '[{"event":"buzz.room.socket_error","reason":"Unexpected server response: 404"}]' > "$T/room.json"
  fake_land
  [ "$status" -eq 1 ]
  [ "$(cat "$T/agents/com.chorus.clearing.plist")" = '<plist>old</plist>' ]
  [ -e "$T/bin/clearing-launch.sh" ]
  echo "$output" | grep -q '404'
}

@test "NEGATIVE PROOF: a room that connects but is never signed in is not proof" {
  echo '[{"event":"buzz.room.connected"}]' > "$T/room.json"
  fake_land
  [ "$status" -eq 1 ]
  [ -e "$T/bin/clearing-launch.sh" ]
}

@test "the land refuses an agent another account owns, and changes nothing" {
  echo '[{"event":"buzz.room.connected"},{"event":"buzz.room.authenticated"}]' > "$T/room.json"
  LAND_AS_UID=99999 fake_land
  [ "$status" -eq 2 ]
  [ "$(cat "$T/agents/com.chorus.clearing.plist")" = '<plist>old</plist>' ]
  [ -e "$T/bin/clearing-launch.sh" ]
}
