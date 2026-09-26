#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake and chorus-principal binaries with stub tmux, claude, ps, token-minter, curl, service probe, osascript and open; no live services, no live panes.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4328 — Jeff 2026-09-26 08:29: "is ur session better now?" It was not: the
# model had landed and nothing wrote it. A login now writes its session (acts
# as, started), a run, a presence and a boot context; each turn keeps last-seen
# current; a delivered nudge makes the presence reachable; logout ends the run.
# Setup is #4295's world, with a curl stub that keeps every body it was sent.
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_AWAKE_BIN:-$ROOT/platform/services/chorus-awake/target/release/chorus-awake}"
  PRINCIPAL="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-awake not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/sessions" "$T/bin" "$T/roles/wren" "$T/roles/kade" "$T/roles/silas" "$T/projects" "$T/identity" "$T/vscode"
  touch "$T/alive-pids"
  cat > "$T/bin/ps" <<EOS
#!/bin/bash
grep -qx "\$2" "$T/alive-pids"
EOS
  # stub tmux: records calls; a session exists once created; send-keys "starts"
  # the role, which registers itself like the real SessionStart hook does
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
case "\$1" in
  has-session) [ -f "$T/tmux-\$3" ]; exit \$? ;;
  new-session) touch "$T/tmux-\$4" ;;
  kill-session) rm -f "$T/tmux-\$3" ;;
  list-clients) cat "$T/clients-\$3" 2>/dev/null ;;
  send-keys)
    role="\${3#chorus-}"; pid=\$(( 800 + \$(ls "$T/sessions" | wc -l) ))
    [ -f "$T/no-register" ] || { printf '{"role":"%s","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%%%s"}' "\$role" "\$pid" "\${pid: -1}" "\${pid: -1}" > "$T/sessions/\$role-\$pid.json"; echo "\$pid" >> "$T/alive-pids"; } ;;
esac
exit 0
EOS
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
[ "\$1" = "agents" ] && echo '[]'
exit 0
EOS
  cat > "$T/bin/token" <<EOS
#!/bin/bash
echo "token \$*" >> "$T/token.log"
[ -f "$T/token-fail" ] && { echo "chorus-identity-token: no credential for '\$1'" >&2; exit 3; }
cat "$T/token-\$1.fixture"
EOS
  # stub curl (#4328): every POST/PUT body is kept as bodies/<n>-<METHOD>-<route>.json;
  # a POST answers {"data":{"name":"<route-kind>-<name sent>"}} the way the API stores it
  mkdir -p "$T/bodies"
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
echo "curl \$*" >> "$T/curl.log"
m=""; b=""; for a in "\$@"; do case "\$a" in POST|PUT) m="\$a" ;; @*.body) b="\${a#@}" ;; esac; done
url="\${@: -1}"
if [ -n "\$m" ]; then
  n=\$(ls "$T/bodies" | wc -l | tr -d ' '); route=\$(echo "\$url" | sed -E 's#.*/v1/##; s#/#_#g')
  cp "\$b" "$T/bodies/\$(printf %03d \$n)-\$m-\$route.json"
  [ "\$m" = POST ] && { kind=\$(echo "\$url" | sed -E 's#.*/##; s#s\$##'); name=\$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "\$b"); printf '{"data":{"name":"%s-%s"}}\n' "\$kind" "\$name"; }
  cat "$T/curl.status" 2>/dev/null || echo 201
else
  cat "$T/row.json" 2>/dev/null
fi
EOS
  # stub service probe: every service answers 200 unless named in down-<name>
  cat > "$T/bin/probe" <<EOS
#!/bin/bash
url="\${@: -1}"; echo "probe \$url" >> "$T/probe.log"
for f in "$T"/down-*; do [ -e "\$f" ] || continue; case "\$url" in *":\${f##*down-}"*) echo 000; exit 7 ;; esac; done
echo 200
EOS
  printf '#!/bin/bash\necho "$*" >> "%s/spine.log"\n' "$T" > "$T/bin/chorus-log"
  printf '#!/bin/bash\necho "osascript $*" >> "%s/osa.log"\n' "$T" > "$T/bin/osascript"
  printf '#!/bin/bash\necho "open $*" >> "%s/open.log"\n' "$T" > "$T/bin/open"
  chmod +x "$T/bin/"*
  for r in wren kade silas; do mk_token "$r"; mkdir -p "$T/identity/$r"; done
  export CLAUDE_BIN="$T/bin/claude" TMUX_BIN="$T/bin/tmux" AWAKE_PS="$T/bin/ps"
  export CHORUS_TOKEN_BIN="$T/bin/token" AWAKE_CURL="$T/bin/curl" CHORUS_LOG_BIN="$T/bin/chorus-log"
  export AWAKE_PROBE_BIN="$T/bin/probe" AWAKE_OSASCRIPT="$T/bin/osascript" AWAKE_OPEN="$T/bin/open"
  export AWAKE_SERVICES="identity=http://stub:3001/,chorus-api=http://stub:3340/h,athena-make=http://stub:3360/s"
  export AWAKE_SERVICE_WAIT=2 AWAKE_NO_RETRY=1
  export CHORUS_IDENTITY_DIR="$T/identity" CHORUS_API_URL="http://stub:3360"
  export CHORUS_SESSIONS_DIR="$T/sessions" AWAKE_ROLES_BASE="$T/roles" CHORUS_ROOT="$ROOT"
  export AWAKE_PROJECTS_DIR="$T/projects" AWAKE_NO_ATTACH=1 AWAKE_WAIT=2 USER=unit-account
  export AWAKE_VSCODE_DIR="$T/vscode" CHORUS_PRINCIPAL_BIN="/h/.chorus/bin/chorus-principal" CHORUS_AWAKE_BIN="$SCRIPT"
  printf '' > "$T/spine-read.log"
  export CHORUS_LOG_FILE="$T/spine-read.log"
  unset TMUX CLAUDECODE CHORUS_ROLE AWAKE_ROLE_DIR
}

mk_token() {
  local role="$1" exp="${2:-$(( $(date +%s) + 600 ))}" payload
  payload=$(printf '{"webid":"https://id.lightlifeurbangardens.com/%s/profile/card#me","jti":"jti-%s-0001","iat":%s,"exp":%s}' "$role" "$role" "$(date +%s)" "$exp" | base64 | tr '+/' '-_' | tr -d '=\n')
  printf 'eyJhbGciOiJFUzI1NiJ9.%s.sig' "$payload" > "$T/token-$role.fixture"
}
# a live, talking session for <role> with <pid>
running() {
  printf '{"role":"%s","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%%0"}' "$1" "$2" "${2: -1}" > "$T/sessions/$1-$2.json"
  echo "$2" >> "$T/alive-pids"; touch "$T/tmux-chorus-$1"
  printf '{"role":"%s","event":"reply.published","timestamp":"%s"}\n' "$1" "$(date '+%Y-%m-%dT%H:%M:%S')" >> "$T/spine-read.log"
}
out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }
state_is() { grep -q "\"state\":\"$2\"" "$T/identity/$1/login.json"; }
body() { cat "$T"/bodies/*-"$1"-"$2".json 2>/dev/null | tail -1; }   # last body sent: METHOD route
bodies() { ls "$T/bodies" | sed 's/^[0-9]*-//'; }
has() { printf '%s' "$1" | grep -qF -- "$2"; }

@test "login writes the session with its role and start, then a run, a presence and a boot context" {
  run "$SCRIPT" on silas
  test "$status" -eq 0
  s=$(body POST identity_sessions); has "$s" '"actsAs":"silas"'; has "$s" '"startedAt":"20'
  r=$(body POST identity_sessionruns); has "$r" '"runOf":"session-silas-'; has "$r" '"ownedBy":"principal-silas"'
  p=$(body POST identity_presences); has "$p" '"presenceOf":"sessionrun-silas-run-'; has "$p" '"reachability":"unknown"'
  c=$(body POST memory_contexts); has "$c" '"contextKind":"boot"'; has "$c" '"contextOf":"sessionrun-silas-run-'
  grep -q "session.run.recorded silas" "$T/spine.log"
}

@test "a second login ends the live run as a restart and names it as previous" {
  run "$SCRIPT" on silas
  first=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/silas/run.row.json")
  # the role dies and is started again
  : > "$T/alive-pids"; rm -f "$T"/sessions/*.json "$T/identity/silas/login.json"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  ended=$(body PUT "identity_sessionruns_$first"); has "$ended" '"endReason":"restart"'; has "$ended" '"runEndedAt":"20'
  new=$(body POST identity_sessionruns); has "$new" "\"previousRun\":\"$first\""
}

@test "NEGATIVE PROOF: a first login names no previous run" {
  run "$SCRIPT" on silas
  r=$(body POST identity_sessionruns)
  has "$r" '"runOf":'   # a run WAS written, so its missing previousRun means something
  test -z "$(printf '%s' "$r" | grep -F previousRun || true)"
}

@test "each turn updates last-seen on the same session row, and names the conversation" {
  run "$SCRIPT" on silas
  sess=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/silas/session.row.json")
  echo '{"session_id":"conv-42","prompt":"work status"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  s=$(body PUT "identity_sessions_$sess"); has "$s" '"lastSeenAt":"20'; has "$s" "\"name\":\"$sess\""
  r=$(bodies | grep -F "PUT-identity_sessionruns_" | tail -1); test -n "$r"
  has "$(cat "$T"/bodies/*"$r")" '"conversationId":"conv-42"'
  # throttled: a second turn inside a minute writes nothing
  n=$(ls "$T/bodies" | wc -l)
  echo '{"session_id":"conv-42","prompt":"and again"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  test "$(ls "$T/bodies" | wc -l)" -eq "$n"
}

@test "a delivered nudge makes the presence reachable; an ordinary turn does not" {
  run "$SCRIPT" on kade
  echo '{"session_id":"c","prompt":"work status"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen kade
  test -z "$(bodies | grep -F PUT-identity_presences || true)"
  echo '{"session_id":"c","prompt":"[nudge from wren | 2026-09-26 09:00 Boston] hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen kade
  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1); has "$p" '"reachability":"reachable"'; has "$p" '"lastDeliveredAt":"20'
}

@test "off ends the run as logout, the presence goes unreachable, then the session closes" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren
  test "$status" -eq 0
  r=$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1); has "$r" '"endReason":"logout"'
  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1); has "$p" '"reachability":"unreachable"'
  s=$(cat "$T"/bodies/*PUT-identity_sessions_* | tail -1); has "$s" '"sessionState":"closed"'
  # the run ends BEFORE the session closes
  test "$(bodies | grep -n PUT-identity_sessionruns | cut -d: -f1)" -lt "$(bodies | grep -n PUT-identity_sessions_ | cut -d: -f1)"
}

@test "NEGATIVE PROOF: a turn after logout does not reopen the session" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren
  n=$(ls "$T/bodies" | wc -l)
  rm -f "$T/identity/wren/seen.at"
  echo '{"session_id":"c","prompt":"late"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen wren
  test "$(ls "$T/bodies" | wc -l)" -eq "$n"
}

@test "sweep closes an expired open session and leaves the live login alone" {
  run "$SCRIPT" on silas
  live=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/silas/session.row.json")
  printf '{"data":[{"name":"session-kade-dead","status":"","actsAs":"","sessionState":"open","expiresAt":"2026-09-01T00:10:00Z","tokenId":"j","ownedBy":"principal-kade"},{"name":"%s","sessionState":"open","expiresAt":"2026-09-01T00:10:00Z","tokenId":"j","ownedBy":"principal-silas"}]}' "$live" > "$T/row.json"
  run "$SCRIPT" sweep
  test "$status" -eq 0
  out_has "1 expired session(s) closed"
  # the listing's "status" and empty fields never reach the PUT (live 09-26: 86 x 422)
  test -z "$(grep -F '"status"' "$T"/bodies/*PUT-identity_sessions_session-kade-dead.json || true)"
  has "$(cat "$T"/bodies/*PUT-identity_sessions_session-kade-dead.json)" '"sessionState":"closed"'
  test -z "$(ls "$T/bodies" | grep -F "PUT-identity_sessions_$live" || true)"
}

@test "a role logged in before #4328 gets its role, start and run on its first turn" {
  running silas 5150
  printf '{"state":"recorded","session":"silas-old-1","pid":5150}' > "$T/identity/silas/login.json"
  printf '{"name":"silas-old-1","tokenId":"j","ownedBy":"principal-silas","sessionState":"open","issuedAt":"2026-09-25T14:54:06Z"}' > "$T/identity/silas/session.row.json"
  echo '{"session_id":"conv-7","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  s=$(body PUT identity_sessions_silas-old-1); has "$s" '"actsAs":"silas"'; has "$s" '"startedAt":"2026-09-25T14:54:06Z"'
  r=$(body POST identity_sessionruns); has "$r" '"runOf":"silas-old-1"'; has "$r" '"conversationId":"conv-7"'
  has "$(body POST identity_presences)" '"presenceOf":"sessionrun-silas-run-'
  has "$(body POST memory_contexts)" '"contextKind":"boot"'
}

@test "NEGATIVE PROOF: a recorded login with nothing running gets no run" {
  printf '{"state":"recorded","session":"silas-old-1","pid":5150}' > "$T/identity/silas/login.json"
  printf '{"name":"silas-old-1","tokenId":"j","ownedBy":"principal-silas","sessionState":"open","issuedAt":"2026-09-25T14:54:06Z"}' > "$T/identity/silas/session.row.json"
  echo '{"session_id":"conv-7","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  test -n "$(bodies | grep -F PUT-identity_sessions_ || true)"
  test -z "$(bodies | grep -F POST-identity_sessionruns || true)"
}

@test "status reads each role's login from the store, not only the local file" {
  running silas 5150
  printf '{"state":"recorded","session":"silas-s1","pid":5150}' > "$T/identity/silas/login.json"
  printf '{"name":"silas-s1"}' > "$T/identity/silas/session.row.json"
  printf '{"data":[{"name":"silas-s1","sessionState":"open","actsAs":"role-silas","startedAt":"2026-09-26T09:00:00Z","lastSeenAt":"2026-09-26T09:05:00Z"}]}' > "$T/row.json"
  run "$SCRIPT" status
  out_has "store: session silas-s1 open, acts as role-silas, since 2026-09-26T09:00:00Z, last seen 2026-09-26T09:05:00Z"
}

# 09-26 08:55 — `on` launched `claude attach <id>`, which never registers (its
# SessionStart fired long ago): "registered NO after 20s", and no window opened.
attach_world() {
  touch "$T/no-register"
  cat > "$T/bin/tmux2" <<EOS
#!/bin/bash
case "\$1" in list-panes) echo "%9 /dev/ttys009 700"; exit 0 ;; esac
exec "$T/bin/tmux" "\$@"
EOS
  printf '#!/bin/bash\n[ "$2" = 700 ] && echo "${PANE_CHILD:-701}"\nexit 0\n' > "$T/bin/pgrep"
  cat > "$T/bin/ps2" <<EOS
#!/bin/bash
if [ "\$1" = "-o" ]; then echo "\${PANE_CMD:-/h/.local/bin/claude attach bda5f062}"; exit 0; fi
grep -qx "\$2" "$T/alive-pids"
EOS
  chmod +x "$T/bin/tmux2" "$T/bin/pgrep" "$T/bin/ps2"
  echo 701 >> "$T/alive-pids"
  export TMUX_BIN="$T/bin/tmux2" AWAKE_PGREP="$T/bin/pgrep" AWAKE_PS="$T/bin/ps2"
}

@test "an attached session that never registers is registered from its pane, and comes up logged in" {
  attach_world
  run "$SCRIPT" on kade
  test "$status" -eq 0
  out_has "pane %9  logged in"
  grep -q '"pid":701' "$T/sessions/kade-701.json"
  grep -q "session.registered.from_pane kade" "$T/spine.log"
  has "$(body POST identity_presences)" '"pane":"%9"'
}

@test "NEGATIVE PROOF: a pane running no claude is not registered, and still says registered NO" {
  attach_world
  export PANE_CMD="-zsh"
  run "$SCRIPT" on kade
  test "$status" -ne 0
  out_has "registered NO"
  test ! -e "$T/sessions/kade-701.json"
}

# ---- #4342: each run writes its Conversation row ----------------------------

@test "#4342 a login whose conversation is not known yet writes no Conversation row (negative proof)" {
  run "$SCRIPT" on silas
  test "$status" -eq 0
  test -z "$(bodies | grep -F POST-memory_conversations || true)"
}

@test "#4342 the first turn names the conversation: one Conversation row, linked to the run" {
  run "$SCRIPT" on silas
  runn=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/silas/run.row.json")
  echo '{"session_id":"4d39d28c-37a5","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  c=$(body POST memory_conversations); has "$c" '"conversationId":"4d39d28c-37a5"'; has "$c" "\"conversationOf\":\"$runn\""; has "$c" '"ownedBy":"principal-silas"'
  grep -q "session.conversation.recorded silas" "$T/spine.log"
  # a later turn in the same conversation writes nothing more
  rm -f "$T/identity/silas/seen.at"
  echo '{"session_id":"4d39d28c-37a5","prompt":"again"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  test "$(bodies | grep -c POST-memory_conversations)" -eq 1
}

@test "#4342 a resumed conversation in a new run moves its row to that run, never a second row" {
  run "$SCRIPT" on silas
  echo '{"session_id":"4d39d28c-37a5","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  # the role restarts and resumes the same transcript; the service already has the row
  : > "$T/alive-pids"; rm -f "$T"/sessions/*.json "$T/identity/silas/login.json" "$T/identity/silas/conversation.row.json"
  run "$SCRIPT" on silas
  echo 409 > "$T/curl.status"
  rm -f "$T/identity/silas/seen.at"
  echo '{"session_id":"4d39d28c-37a5","prompt":"back"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  newrun=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/silas/run.row.json")
  p=$(cat "$T"/bodies/*PUT-memory_conversations_* | tail -1); has "$p" "\"conversationOf\":\"$newrun\""
}

@test "#4343 a login saved with actsAs role-wren is sent back as wren, so the update is not refused" {
  running wren 5151
  printf '{"state":"recorded","session":"wren-old","pid":5151}' > "$T/identity/wren/login.json"
  printf '{"name":"wren-old","tokenId":"j","ownedBy":"principal-wren","sessionState":"open","issuedAt":"2026-09-26T15:06:25Z","actsAs":"role-wren","startedAt":"2026-09-26T15:06:25Z"}' > "$T/identity/wren/session.row.json"
  echo '{"session_id":"c","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen wren
  s=$(body PUT identity_sessions_wren-old); has "$s" '"actsAs":"wren"'
  test -z "$(printf '%s' "$s" | grep -F '"actsAs":"role-wren"' || true)"
}

@test "#4344 a login writes the role's credential files as rows that name the file and never carry the secret" {
  printf '{"secret":"SUPERSECRET-client-secret","issuer":"x"}' > "$T/identity/silas/cred.json"
  printf '{"seckey":"nsec1supersecretkey","pubkey":"ab"}' > "$T/identity/silas/nostr.json"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  test "$(bodies | grep -c POST-security_credentials)" -eq 2
  c=$(cat "$T"/bodies/*POST-security_credentials*); has "$c" '"credentialKind":"css-client"'; has "$c" '"credentialKind":"nostr-key"'; has "$c" '/silas/cred.json"'
  # negative proof: no secret value reaches any body
  test -z "$(cat "$T"/bodies/* | grep -F -e SUPERSECRET -e nsec1 || true)"
}

@test "#4344 an unchanged credential is not written again" {
  printf '{"secret":"s"}' > "$T/identity/silas/cred.json"
  run "$SCRIPT" on silas
  n=$(bodies | grep -c security_credentials)
  echo '{"session_id":"c","prompt":"hi"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen silas
  test "$(bodies | grep -c security_credentials)" -eq "$n"
}
