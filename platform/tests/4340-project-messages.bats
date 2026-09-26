#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake and chorus-principal binaries with stub tmux, claude, ps, token-minter, curl, service probe, osascript and open; no live services, no live panes.
#
# #4340 — messages.db into the model. Jeff 2026-09-26: messages, channel, and
# "even a migration to buzz"; option A. The projector reads what pulse recorded
# and writes a Message row per message, a Delivery row per attempt, over one of
# three Channel rows, and marks a delivery "truncated" when the recipient's own
# turn received less than was sent (Wren's 12:45 answer). Stubs: sqlite3 serves
# a fixture, curl keeps every body and serves each listing from a fixture file.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_AWAKE_BIN:-$ROOT/platform/services/chorus-awake/target/release/chorus-awake}"
  [ -x "$SCRIPT" ] || skip "chorus-awake not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/bin" "$T/bodies" "$T/identity/silas" "$T/identity/wren"
  printf '#!/bin/bash\necho token-$1\n' > "$T/bin/token"
  printf '#!/bin/bash\necho "$*" >> "%s/spine.log"\n' "$T" > "$T/bin/chorus-log"
  cat > "$T/bin/sqlite3" <<EOS
#!/bin/bash
echo "\$*" >> "$T/sqlite.log"
cat "$T/messages.json"
EOS
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
m=""; b=""; for a in "\$@"; do case "\$a" in POST|PUT) m="\$a" ;; @*.body) b="\${a#@}" ;; esac; done
url="\${@: -1}"
if [ -n "\$m" ] && grep -qE '"(sentBy|sentTo)":"principal-|"deliveryOf":"message-|"actsAs":"role-' "\$b"; then
  printf '{"error":"validation","message":"double-prefix"}\n422\n'; exit 0
fi
if [ -n "\$m" ]; then
  n=\$(ls "$T/bodies" | wc -l | tr -d ' '); route=\$(echo "\$url" | sed -E 's#.*/v1/##; s#/#_#g')
  cp "\$b" "$T/bodies/\$(printf %03d \$n)-\$m-\$route.json"
  [ "\$m" = POST ] && { kind=\$(echo "\$url" | sed -E 's#.*/##; s#s\$##; s#ie\$#y#'); name=\$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "\$b"); printf '{"data":{"name":"%s-%s"}}\n' "\$kind" "\$name"; }
  echo 201
else
  r=\$(echo "\$url" | sed -E 's#.*/v1/##; s#\?.*##; s#/#_#g'); cat "$T/list-\$r.json" 2>/dev/null || echo '{"data":[]}'
fi
EOS
  chmod +x "$T/bin/"*
  echo '{"data":[{"name":"jeff"},{"name":"kade"},{"name":"silas"},{"name":"wren"}]}' > "$T/list-identity_principals.json"
  echo '{"data":[{"name":"wren-s1","ownedBy":"principal-wren","startedAt":"2026-09-26T16:28:25Z","endedAt":""}]}' > "$T/list-identity_sessions.json"
  echo '{"data":[{"name":"silas-run-b","ownedBy":"principal-silas","startedAt":"2026-09-26T16:30:00Z","runEndedAt":""}]}' > "$T/list-identity_sessionruns.json"
  echo '{"data":[{"name":"silas-presence-b","presenceOf":"session-run-silas-run-b"}]}' > "$T/list-identity_presences.json"
  cat > "$T/messages.json" <<'JSON'
[{"id":501,"type":"nudge","from":"wren","to":"silas","content":"[nudge from wren | 12:45] My answers.\n1 Channel = transport\n5 Buzz is then: add one Channel row","created_at":"2026-09-26 16:45:32","delivery_status":"delivered","delivered_at":"2026-09-26 16:45:33","last_delivery_error":null},
 {"id":502,"type":"nudge","from":"system","to":"silas","content":"chorus-health: fuseki-memory","created_at":"2026-09-26 16:49:00","delivery_status":"delivered","delivered_at":"2026-09-26 16:49:01","last_delivery_error":null},
 {"id":503,"type":"nudge","from":"silas","to":"kade","content":"hello kade","created_at":"2026-09-26 16:50:00","delivery_status":"pending","delivered_at":null,"last_delivery_error":null}]
JSON
  export CHORUS_TOKEN_BIN="$T/bin/token" AWAKE_CURL="$T/bin/curl" CHORUS_LOG_BIN="$T/bin/chorus-log" AWAKE_SQLITE="$T/bin/sqlite3"
  export CHORUS_IDENTITY_DIR="$T/identity" CHORUS_API_URL="http://stub:3360" CHORUS_MESSAGES_DB="$T/m.db" CHORUS_ROOT="$ROOT"
  unset CLAUDECODE CHORUS_ROLE
}
body() { cat "$T"/bodies/*-"$1"-"$2".json 2>/dev/null | tail -1; }
# the one body file that carries <text> (bodies have no trailing newline, so never cat them together)
one() { local f; f=$(grep -lF -- "$2" "$T"/bodies/*"$1"*.json 2>/dev/null | tail -1); [ -n "$f" ] && cat "$f"; }
has() { printf '%s' "$1" | grep -qF -- "$2"; }
lacks() { test -z "$(printf '%s' "$1" | grep -F -- "$2" || true)"; }

@test "the three channels are created once" {
  run "$SCRIPT" project-messages
  test "$status" -eq 0
  test "$(ls "$T/bodies" | grep -c POST-messages_channels)" -eq 3
  echo '{"data":[{"channelKind":"terminal"},{"channelKind":"nudge"},{"channelKind":"clearing"}]}' > "$T/list-messages_channels.json"
  n=$(ls "$T/bodies" | wc -l)
  echo '[]' > "$T/messages.json"
  run "$SCRIPT" project-messages
  test "$(ls "$T/bodies" | grep -c POST-messages_channels)" -eq 3
}

@test "a peer message is written with its principals and session, and its delivery names the presence" {
  run "$SCRIPT" project-messages
  test "$status" -eq 0
  m=$(one POST-messages_messages '"sourceId":"501"')
  has "$m" '"sentBy":"wren"'; has "$m" '"sentTo":"silas"'; has "$m" '"sentInSession":"wren-s1"'; has "$m" '"overChannel":"nudge"'
  d=$(one POST-messages_deliveries 'message 501')
  has "$d" '"deliveredTo":"silas-presence-b"'; has "$d" '"deliveryOf":"src-501"'
}

@test "an alert from a machine keeps its sender as provenance, with no principal" {
  run "$SCRIPT" project-messages
  m=$(one POST-messages_messages '"sourceId":"502"')
  has "$m" '"senderName":"system"'; lacks "$m" '"sentBy"'
}

@test "a message that reached the pane cut short is recorded as truncated" {
  printf '{"at":"2026-09-26T16:45:33Z","text":"5 Buzz is then: add one Channel row"}\n' > "$T/identity/silas/received.jsonl"
  run "$SCRIPT" project-messages
  d=$(one POST-messages_deliveries 'message 501')
  has "$d" '"deliveryOutcome":"truncated"'
  grep -q "message.delivery.truncated id=501" "$T/spine.log"
}

@test "NEGATIVE PROOF: the same message received whole is delivered, not truncated" {
  python3 -c 'import json;print(json.dumps({"at":"2026-09-26T16:45:33Z","text":json.load(open("'"$T"'/messages.json"))[0]["content"]}))' > "$T/identity/silas/received.jsonl"
  run "$SCRIPT" project-messages
  d=$(one POST-messages_deliveries 'message 501')
  has "$d" '"deliveryOutcome":"delivered"'
}

@test "a pending delivery stays open and is updated in place when it lands; nothing is written twice" {
  run "$SCRIPT" project-messages
  has "$(one POST-messages_deliveries 'message 503')" '"deliveryOutcome":"pending"'
  msgs=$(ls "$T/bodies" | grep -c POST-messages_messages)
  sed -i '' 's/"delivery_status":"pending","delivered_at":null/"delivery_status":"delivered","delivered_at":"2026-09-26 16:50:05"/' "$T/messages.json"
  run "$SCRIPT" project-messages
  test "$(ls "$T/bodies" | grep -c POST-messages_messages)" -eq "$msgs"
  has "$(body PUT messages_deliveries_delivery-src-503)" '"deliveryOutcome":"delivered"'
}

@test "the seen hook keeps what arrived, and only the last 200 prompts" {
  for i in $(seq 1 205); do echo "{\"session_id\":\"c\",\"prompt\":\"p$i\"}" | AWAKE_SEEN_EVERY=999999 "$SCRIPT" seen wren; done
  test "$(wc -l < "$T/identity/wren/received.jsonl" | tr -d ' ')" -eq 200
  tail -1 "$T/identity/wren/received.jsonl" | grep -qF '"text":"p205"'
}

@test "NEGATIVE PROOF: a short prompt from before the message was sent never marks it truncated" {
  printf '{"at":"2026-09-26T16:40:00Z","text":"5 Buzz is then: add one Channel row"}\n' > "$T/identity/silas/received.jsonl"
  run "$SCRIPT" project-messages
  has "$(one POST-messages_deliveries 'message 501')" '"deliveryOutcome":"delivered"'
}

@test "NEGATIVE PROOF: the stub refuses a double prefix the way the service does, so a prefixed edge cannot pass here" {
  printf '{"name":"x","sentBy":"principal-wren"}' > "$T/probe.body"
  run "$T/bin/curl" -s -X POST --data-binary "@$T/probe.body" http://stub:3360/v1/messages/messages
  printf '%s' "$output" | grep -q 422
}

@test "a message the service refused is tried again on the next pass, never skipped by the watermark" {
  # the stub refuses message 502 once (a timeout on the live run, 09-26 14:27, lost message 40169)
  sed -i '' 's/"content":"chorus-health: fuseki-memory"/"content":"chorus-health: fuseki-memory","sentBy_bad":1/' "$T/messages.json"
  cat > "$T/bin/curl.orig" < "$T/bin/curl"
  { echo '#!/bin/bash'; echo 'for a in "$@"; do case "$a" in @*.body) b="${a#@}";; esac; done'; echo "if [ -n \"\$b\" ] && grep -q '\"sourceId\":\"502\"' \"\$b\" && [ ! -f $T/refused-once ]; then touch $T/refused-once; printf '\\n000\\n'; exit 0; fi"; echo "exec $T/bin/curl.orig \"\$@\""; } > "$T/bin/curl"
  chmod +x "$T/bin/curl" "$T/bin/curl.orig"
  run "$SCRIPT" project-messages
  out_503=$(ls "$T/bodies" | grep -c POST-messages_messages)
  test -z "$(grep -lF '"sourceId":"502"' "$T"/bodies/*POST-messages_messages.json 2>/dev/null || true)"
  run "$SCRIPT" project-messages
  grep -lqF '"sourceId":"502"' "$T"/bodies/*POST-messages_messages.json
}
