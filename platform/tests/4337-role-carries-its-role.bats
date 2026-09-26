#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake and chorus-principal binaries with stub tmux, claude, ps, token-minter, curl, service probe, osascript and open; no live services, no live panes.
#
# #4337 — Jeff 2026-09-26: "i dont want to have to disassemble and reassemble the
# car evertime i want to turn an agent on or off"; "i dont want the 10 steps i
# need to run when the 1 step command fails". That morning Kade's session ran as
# Wren: `claude attach` put it inside the Claude daemon's warm spare, started from
# Wren's pane with her env. The fix takes the layer away: roles run with no
# daemon (disableAgentView), `on` resumes a background conversation in its own
# pane instead of attaching, `off` ends the background copy, and a process that
# carries another role is never called logged in.
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

@test "the launch turns the daemon off and names the role, and nothing is attached" {
  run "$SCRIPT" on kade
  test "$status" -eq 0
  grep -qF "CLAUDE_CODE_DISABLE_AGENT_VIEW=1" "$T/tmux.log"
  grep -qF "CHORUS_ROLE='kade'" "$T/tmux.log"
  test -z "$(grep -F "claude attach" "$T/tmux.log" || true)"
}

@test "every role's settings turn Claude's background daemon off" {
  for r in wren kade silas; do
    python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("disableAgentView") is True else 1)' "$ROOT/roles/$r/.claude/settings.json"
  done
}

@test "off ends the role's background copy of the conversation" {
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
[ "\$1" = "agents" ] && echo '[{"id":"bda5f062","kind":"background","sessionId":"bda5f062-da2b"},{"id":"x1","kind":"interactive"}]'
exit 0
EOS
  chmod +x "$T/bin/claude"
  running kade 4264
  run "$SCRIPT" off kade
  test "$status" -eq 0
  grep -qx "claude stop bda5f062" "$T/claude.log"
  test -z "$(grep -x "claude stop x1" "$T/claude.log" || true)"
}

wrong_env() {  # ps that answers `eww` with the given CHORUS_ROLE
  cat > "$T/bin/ps3" <<EOS
#!/bin/bash
if [ "\$1" = "eww" ]; then echo "/h/.local/bin/claude -c PWD=/x CHORUS_ROLE=$1 TERM=xterm"; exit 0; fi
grep -qx "\$2" "$T/alive-pids"
EOS
  chmod +x "$T/bin/ps3"; export AWAKE_PS="$T/bin/ps3"
}

@test "a process carrying another role is refused as WRONG ROLE, never 'logged in'" {
  wrong_env wren
  run "$SCRIPT" on kade
  test "$status" -ne 0
  out_has "WRONG ROLE"
  out_has "runs as wren"
  out_has "Run: chorus-principal off kade && chorus-principal on kade"
  out_lacks "logged in  via"
  grep -q "session.wrong_role kade" "$T/spine.log"
}

@test "NEGATIVE PROOF: the same check passes a process that carries its own role" {
  wrong_env kade
  run "$SCRIPT" on kade
  test "$status" -eq 0
  out_has "logged in"
  out_lacks "WRONG ROLE"
}
