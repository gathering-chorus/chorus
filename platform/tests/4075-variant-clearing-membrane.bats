#!/usr/bin/env bats
# @test-type: integration — starts the Clearing from THIS tree on a free port with a
# fixture "prod" store; no live service, no ~/.chorus, no /tmp/bridge-messages.json.
#
# #4075 — a message posted in the variant Clearing lands in the VARIANT store and
# leaves prod's row count unchanged. Silas's gate ask 2026-09-02: "post one message
# in the variant Clearing and count prod rows before and after ... and it FAILS
# when the variant is pointed at prod." Test 2 is that failure, shown.
#
# Brings its own world (#3528): CHORUS_HOME, the message store, the spine and the
# "prod" store are all under $BATS_TEST_TMPDIR.

ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)}"
# node from PATH (the nightly and the werk both export it); no absolute home path (#4075 nightly red, Silas 19:19)
NODE="${NODE:-$(command -v node)}"

setup() {
  W="$BATS_TEST_TMPDIR"
  export PROD_STORE="$W/prod-bridge-messages.json"   # stands in for /tmp/bridge-messages.json
  export WERK_STORE="$W/werk/.chorus-demo/bridge-messages.json"
  mkdir -p "$W/werk/.chorus-demo" "$W/home"
  printf '[{"from":"jeff","text":"p1"},{"from":"kade","text":"p2"},{"from":"jeff","text":"p3"}]' > "$PROD_STORE"
  PORT=$(( 3600 + RANDOM % 300 ))
  [ -f "$ROOT/directing/clearing/dist/server.js" ] || skip "build directing/clearing first"
}

# start_clearing <msg-file> ; sets PID, TOKEN
start_clearing() {
  local store="$1"
  ( cd "$ROOT/directing/clearing" && \
    HOME="$W/home" COMMAND_CHANNEL_PORT="$PORT" CLEARING_HTTPS_PORT=0 \
    CHORUS_API_URL="http://127.0.0.1:9" CHORUS_API_BASE="http://127.0.0.1:9" PULSE_URL="http://127.0.0.1:9" \
    CHORUS_LOG_FILE="$W/spine.log" CLEARING_SPINE_FILE="$W/spine.log" CLEARING_MSG_FILE="$store" \
    CHORUS_CLEARING_REQUIRE_DPOP=0 BUZZ_ROOM_ENABLED=0 \
    exec "$NODE" dist/server.js </dev/null >"$W/daemon.log" 2>&1 3>&- ) &
  PID=$!
  echo "$PID" > "$W/pid"
  for _ in $(seq 1 60); do
    curl -sf --max-time 1 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
    sleep 0.5
  done
  curl -sf --max-time 1 "http://127.0.0.1:$PORT/health" >/dev/null || { cat "$W/daemon.log"; return 1; }
  TOKEN=$(sed -n 's/.*remote access token: \([0-9a-f]*\).*/\1/p' "$W/daemon.log" | head -1)
  [ -n "$TOKEN" ]
}

teardown() {
  # own-world process, started by this test (#3528): stopping it is teardown, not ops.
  if [ -f "$W/pid" ]; then kill "$(cat "$W/pid")" 2>/dev/null || true; fi
  true
}

post_one() {
  curl -sf --max-time 5 -X POST "http://127.0.0.1:$PORT/api/message" \
    -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
    -d '{"from":"wren","text":"#4075 membrane probe"}' >/dev/null
}

rows() { "$NODE" -e 'const a=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));console.log(a.length)' "$1"; }

@test "variant Clearing: one message posted lands in the werk store; prod rows unchanged" {
  start_clearing "$WERK_STORE"
  before=$(rows "$PROD_STORE")
  post_one
  sleep 11   # the room persists every 10s
  [ "$(rows "$PROD_STORE")" -eq "$before" ]
  [ -f "$WERK_STORE" ]
  grep -q '#4075 membrane probe' "$WERK_STORE"
  ! grep -q '#4075 membrane probe' "$PROD_STORE"
}

@test "NEGATIVE PROOF (#3734): the same room pointed at the prod store DOES change prod rows — the check above can go red" {
  start_clearing "$PROD_STORE"
  before=$(rows "$PROD_STORE")
  post_one
  sleep 11
  after=$(rows "$PROD_STORE")
  [ "$after" -gt "$before" ]                       # prod grew: the assertion in test 1 would FAIL here
  grep -q '#4075 membrane probe' "$PROD_STORE"
}
