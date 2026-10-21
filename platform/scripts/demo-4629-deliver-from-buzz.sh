#!/bin/bash
# @domain: messages
# #4629 AC2 demo — one nudge, signed by its sender, goes to the STAGING Buzz room,
# and the werk's demo pulse delivers it FROM the room, exactly once.
# Runs the WERK-built nudge-send (never ~/.chorus/bin, which has no signing code:
# it would pass vacuously through the fallback, Silas 06:51) and asserts the
# release cause is "room", not "fallback".
#
#   demo-4629-deliver-from-buzz.sh <werk-root> <demo-pulse-url> <door-url> <api-url>
# The staging relay and channel come from the werk's clearing-v2.config.json
# (host of buzzRelayHost on stagingRelayPort, stagingTeamChannel), as the demo
# pulse's own env does: no relay address is written here (#3370).
set -u
WERK="${1:?werk root}"; PULSE="${2:?demo pulse url}"; DOOR="${3:?door url}"; API="${4:?api url}"
STORE="$WERK/.chorus-demo"
SHIM="$WERK/platform/services/chorus-hooks/target/release/chorus-hook-shim"
CFG="$WERK/directing/clearing-v2/clearing-v2.config.json"
fail() { echo "AC2 FAIL: $*"; exit 1; }
[ -x "$SHIM" ] || fail "no werk-built chorus-hook-shim at $SHIM (cargo build --release in chorus-hooks)"
read -r RELAY CHANNEL < <(python3 - "$CFG" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
host = c.get('buzzRelayHost', '').split(':')[0]
port, chan = c.get('stagingRelayPort'), c.get('stagingTeamChannel')
print(f"ws://{host}:{port} {chan}" if host and port and chan else "")
PY
)
[ -n "${RELAY:-}" ] && [ -n "${CHANNEL:-}" ] || fail "no staging room in $CFG"
case "$RELAY" in *:13000*) fail "refusing the real room ($RELAY)";; esac

PROBE="AC2 probe $(uuidgen | tr 'A-Z' 'a-z')"
OUT=$(CHORUS_NUDGE_SIGNS=1 BUZZ_RELAY_URL="$RELAY" BUZZ_TEAM_CHANNEL="$CHANNEL" \
      CHORUS_PULSE_URL="$PULSE/api/nudge" ATHENA_MAKE_URL="$DOOR" CHORUS_API_URL="$API" \
      "$SHIM" nudge-send wren silas "$PROBE" 2>&1) || fail "nudge-send: $OUT"
echo "$OUT"
TRACE=$(echo "$OUT" | sed -n 's/.*(trace=\([0-9a-f-]\{36\}\)).*/\1/p')
[ -n "$TRACE" ] || fail "no trace in the send line"
echo "$OUT" | grep -q "Buzz: stored" || fail "the sender's note did not reach the staging room"

ROW=$(sqlite3 -readonly "$STORE/messages.db" "SELECT id FROM messages WHERE trace_id='$TRACE'")
[ -n "$ROW" ] || fail "the demo pulse stored no row for trace $TRACE"
# pulse's spine lines are JSON: {"event":"buzz.delivery.released"|"buzz.delivery.fallback","id":"<row>","cause":...}
counts() { python3 - "$STORE/spine.log" "$ROW" <<'PY'
import json, sys
room = fb = 0
try:
    lines = open(sys.argv[1]).readlines()
except OSError:
    lines = []
for line in lines:
    try: e = json.loads(line)
    except ValueError: continue
    if str(e.get('id')) != sys.argv[2]: continue
    if e.get('event') == 'buzz.delivery.released' and e.get('cause') == 'room': room += 1
    if e.get('event') == 'buzz.delivery.fallback': fb += 1
print(room, fb)
PY
}
# the demo pulse releases this row: cause must be room, within 20 s (the fallback is 30 s), once
ROOM=0; FALLBACK=0
for _ in $(seq 1 40); do
  read -r ROOM FALLBACK < <(counts)
  [ "$ROOM" != "0" ] || [ "$FALLBACK" != "0" ] && break
  sleep 0.5
done
[ "$FALLBACK" = "0" ] || fail "row $ROW was released by the FALLBACK, not the room"
[ "$ROOM" = "1" ] || fail "row $ROW released from the room $ROOM times (want exactly 1)"
sleep 2
DELIVERED=$(grep -c "$PROBE" "$STORE/deliveries.log" 2>/dev/null || true)
[ "$DELIVERED" = "1" ] || fail "the stub delivered the probe $DELIVERED times (want 1)"
echo "AC2 PASS: trace $TRACE → staging room → demo pulse released row $ROW from the room → delivered once"
