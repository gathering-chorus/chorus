#!/usr/bin/env bash
# @domain: identity — the product domain this suite guards (#4334)
# login-harness.bash — #4367. The fixture world the login scenarios run in: stub
# tmux, claude, ps, token minter, curl, service probe, osascript and open, all
# writing into $T; no live services, no live panes. Moved out of
# 4328-session-rows.bats so the bats cases and the cucumber login steps
# (features/step_definitions/login_steps.ts) stand in the same world.
#   login_harness_stubs   write the stubs into $T (once per case / scenario)
#   login_harness_env     export the env the binaries read (every shell)
# Needs T (a scratch dir) and ROOT (the repo) set by the caller.

login_harness_stubs() {
  mkdir -p "$T/sessions" "$T/bin" "$T/roles/wren" "$T/roles/kade" "$T/roles/silas" "$T/projects" "$T/identity" "$T/vscode"
  touch "$T/alive-pids"
  cat > "$T/bin/ps" <<EOS
#!/bin/bash
grep -qx "\$2" "$T/alive-pids"
EOS
  # stub tmux: records calls; a session exists once created; the launch "starts"
  # the role, which registers itself like the real SessionStart hook does.
  # #4813 (2026-11-01) launches with one chained call (new-session ; set-option ;
  # respawn-pane -t <session> ... <launch>), not send-keys: the stub registers on
  # respawn-pane too, or every login here reads "registered NO" (#4505 run 2).
  # #4933 — what a role's pane does first: its wrapper runs `establish <role> --run
  # <stamp>` as the role, exactly as the launch line names it (any suite's own tmux
  # stub calls this on respawn-pane). In a real pane the wrapper inherits that pane's
  # id: here it is the world's pane, or the id the default stub registers it under.
  cat > "$T/bin/pane-establish" <<EOS
#!/bin/bash
args=("\$@"); last="\${args[\${#args[@]}-1]}"
est=\$(printf '%s' "\$last" | grep -oE "'[^']+' establish '[^']+' --run '[0-9a-f]+'" | head -1)
[ -n "\$est" ] || exit 0
eval "set -- \$est"
pane=\$("\${TMUX_BIN:-$T/bin/tmux}" list-panes 2>/dev/null | awk '{print \$1}' | head -1)
[ -n "\$pane" ] || { n=\$(( 800 + \$(ls "$T/sessions" | wc -l) )); pane="%\${n: -1}"; }
TMUX_PANE="\$pane" "\${LOGIN_HARNESS_PRINCIPAL:-\$1}" establish "\$3" --run "\$5" >> "$T/establish.out" 2>> "$T/establish.log"
EOS
  chmod +x "$T/bin/pane-establish"
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
register() { role="\${1#chorus-}"; pid=\$(( 800 + \$(ls "$T/sessions" | wc -l) ))
  [ -f "$T/no-register" ] || { printf '{"role":"%s","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%%%s"}' "\$role" "\$pid" "\${pid: -1}" "\${pid: -1}" > "$T/sessions/\$role-\$pid.json"; echo "\$pid" >> "$T/alive-pids"; }; }
args=("\$@"); for ((i=0; i<\${#args[@]}; i++)); do
  [ "\${args[i]}" = respawn-pane ] || continue
  # #4933 — the pane runs the role's wrapper (bin/pane-establish); refused, the
  # runtime never starts, so nothing registers
  "$T/bin/pane-establish" "\$@" || exit 0
  for ((j=i+1; j<\${#args[@]}; j++)); do [ "\${args[j]}" = -t ] && { touch "$T/tmux-\${args[j+1]}"; register "\${args[j+1]}"; exit 0; }; done
done
case "\$1" in
  has-session) [ -f "$T/tmux-\$3" ]; exit \$? ;;
  new-session) touch "$T/tmux-\$4" ;;
  kill-session) rm -f "$T/tmux-\$3" ;;
  list-clients) cat "$T/clients-\$3" 2>/dev/null ;;
  send-keys) register "\$3" ;;
esac
exit 0
EOS
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
# agents: the fixture list (agents.json), or a failure when agents-fail exists (#4184)
if [ "\$1" = "agents" ]; then [ -f "$T/agents-fail" ] && { echo "boom: unknown option --cwd" >&2; exit 1; }; cat "$T/agents.json" 2>/dev/null || echo '[]'; fi
exit 0
EOS
  cat > "$T/bin/token" <<EOS
#!/bin/bash
echo "token \$*" >> "$T/token.log"
[ -f "$T/token-slow" ] && sleep "\$(cat "$T/token-slow")"   # #4403: widen a race on purpose
[ -f "$T/token-fail" ] && { echo "chorus-identity-token: no credential for '\$1'" >&2; exit 3; }
[ -f "$T/down-3001" ] && { echo "chorus-identity-token: identity not answering (:3001)" >&2; exit 3; }   # #4933: the issuer is identity
[ -f "$T/token.fixture" ] && { cat "$T/token.fixture"; exit 0; }   # one token for every role, when a case sets it
cat "$T/token-\$1.fixture"
EOS
  # stub curl (#4328): every POST/PUT body is kept as bodies/<n>-<METHOD>-<route>.json;
  # a POST answers {"data":{"name":"<route-kind>-<name sent>"}} the way the API stores it
  mkdir -p "$T/bodies"
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
echo "curl \$*" >> "$T/curl.log"
case "\${@: -1}" in *"/v1/identity/sessions?channel=browser"*) if [ -f "$T/api-down" ]; then printf '\n000\n'; elif [ -f "$T/nobody-signed-in" ]; then printf '{"data":[]}\n200\n'; else printf '{"data":[{"name":"jeff-browser-1","ownedBy":"principal-jeff","actsAs":"jeff","channel":"browser","sessionState":"open","expiresAt":"2099-01-01T00:00:00Z"}]}\n200\n'; fi; exit 0 ;; esac   # #4412: who is signed in
case "\${@: -1}" in */v1/roles/roles\?*) if [ -f "$T/roles-door-down" ]; then printf 'upstream down\n'; elif [ -f "$T/roles-door.json" ]; then cat "$T/roles-door.json"; else printf '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"},{"name":"abby-normal","roleKind":"agent"},{"name":"jeff","roleKind":"human"}]}\n'; fi; exit 0 ;; esac
case "\${@: -1}" in */v1/identity/principals\?*) acct=jeff-fixture-account; [ -f "$T/jeff-at-terminal" ] && acct=\$USER; printf '{"data":[{"name":"jeff","principalKind":"person","hostAccount":"%s"},{"name":"wren","principalKind":"agent","hostAccount":"%s"}]}\n' "\$acct" "\$USER"; exit 0 ;; esac   # #4412: who is at this terminal
case "\${@: -1}" in */v1/identity/principals/*) pn="\${@: -1}"; pn="\${pn##*/}"; if [ -f "$T/principal-\$pn.json" ]; then cat "$T/principal-\$pn.json"; else case "\$pn" in wren|silas|kade|borg-queen|abby-normal) printf '{"data":{"principalKind":"agent","hostAccount":"chorus-%s"}}\n200\n' "\$pn" ;; jeff) printf '{"data":{"principalKind":"person","hostAccount":"jeffbridwell"}}\n200\n' ;; *) printf '{"data":{"status":404}}\n404\n' ;; esac; fi; exit 0 ;; esac   # #4368: the Principal row login reads
case "\$* \${@: -1}" in *" PUT "*|*" POST "*) ;; *"/v1/identity/sessionruns/"*) rn="\${@: -1}"; rn="\${rn##*/}"; hit=\$(grep -l "\"name\": *\"\$rn\"" "$T"/bodies/*-POST-identity_sessionruns.json 2>/dev/null | tail -1); if [ -n "\$hit" ]; then printf '{"data":%s}\n200\n' "\$(cat "\$hit")"; else printf '{"data":{}}\n404\n'; fi; exit 0 ;; *"/v1/identity/presences/"*) [ -f "$T/presence-door-down" ] && exit 22; last=\$(ls "$T/bodies" | grep -i presences | tail -1); if [ -n "\$last" ]; then printf '{"data":%s}\n' "\$(cat "$T/bodies/\$last")"; else printf '{"data":{}}\n'; fi; exit 0 ;; esac   # #4895: a Presence write reads the door's current row first; the door holds the last one written
m=""; b=""; for a in "\$@"; do case "\$a" in POST|PUT) m="\$a" ;; @*.body) b="\${a#@}" ;; esac; done
url="\${@: -1}"
if [ -n "\$m" ]; then
  n=\$(ls "$T/bodies" | wc -l | tr -d ' '); route=\$(echo "\$url" | sed -E 's#.*/v1/##; s#/#_#g')
  cp "\$b" "$T/bodies/\$(printf %03d \$n)-\$m-\$route.json"
  case "\$b" in *session.body) cp "\$b" "$T/curl.body" ;; esac   # the last Session body, by its old name
  [ "\$m" = POST ] && { kind=\$(echo "\$url" | sed -E 's#.*/##; s#s\$##'); name=\$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "\$b"); printf '{"data":{"name":"%s-%s"}}\n' "\$kind" "\$name"; }
  cat "$T/curl.reply" 2>/dev/null   # #4367: a refusal's body, when a case sets one
  # #4933 — a status for one route only (curl.status.<route>, e.g. identity_sessions), else for all
  cat "$T/curl.status.\$route" 2>/dev/null || cat "$T/curl.status" 2>/dev/null || echo 201
else
  cat "$T/existing.json" 2>/dev/null || cat "$T/row.json" 2>/dev/null   # existing.json: the row a 409 re-read finds (#4215)
fi
EOS
  # stub service probe: every service answers 200 unless named in down-<name>
  cat > "$T/bin/probe" <<EOS
#!/bin/bash
url="\${@: -1}"; echo "probe \$url" >> "$T/probe.log"
for f in "$T"/down-*; do [ -e "\$f" ] || continue; case "\$url" in *":\${f##*down-}"*) echo 000; exit 7 ;; esac; done
echo 200
EOS
  # #4383 stub sudo: records the call; `-n -u <acct> [-H] cmd…` runs cmd as
  # this user (the harness has no second account), refused when $T/sudo-deny exists
  cat > "$T/bin/sudo" <<EOS
#!/bin/bash
DIR="\$(cd "\$(dirname "\$0")" && pwd)"
echo "sudo \$*" >> "$T/sudo.log"
[ -f "$T/sudo-deny" ] && { echo "sudo: a password is required" >&2; exit 1; }
while [ \$# -gt 0 ]; do case "\$1" in -n|-H) shift ;; -u) shift 2 ;; *) break ;; esac; done
case "\$1" in /bin/launchctl) shift; set -- "\$DIR/launchctl" "\$@" ;; esac
exec "\$@"
EOS
  cat > "$T/bin/launchctl" <<EOS
#!/bin/bash
DIR="\$(cd "\$(dirname "\$0")" && pwd)"
echo "launchctl \$*" >> "$T/launchctl.log"
[ -f "$T/launchctl-deny" ] && { echo "launchctl: Operation not permitted" >&2; exit 1; }
while [ \$# -gt 0 ]; do case "\$1" in asuser) shift 2 ;; *) break ;; esac; done
case "\$1" in /usr/bin/sudo) shift; set -- "\$DIR/sudo" "\$@" ;; esac
exec "\$@"
EOS
  # #4445 stub relay projection: records the call, never reaches the relay;
  # fails like a dropped ssh when $T/relay-fail exists
  cat > "$T/bin/relay" <<EOS
#!/bin/bash
echo "relay \$*" >> "$T/relay.log"
[ -f "$T/relay-fail" ] && { echo "ssh: connect to host 192.0.2.1 port 22: Operation timed out" >&2; exit 255; }
echo "projected: graph=4 keys, +1 -0 (relay allowlist = allow-set projection)"
EOS
  printf '#!/bin/bash\necho "$*" >> "%s/spine.log"\n' "$T" > "$T/bin/chorus-log"
  printf '#!/bin/bash\necho "osascript $*" >> "%s/osa.log"\n' "$T" > "$T/bin/osascript"
  printf '#!/bin/bash\necho "open $*" >> "%s/open.log"\n' "$T" > "$T/bin/open"
  chmod +x "$T/bin/"*
  for r in wren kade silas; do mk_token "$r"; mkdir -p "$T/identity/$r"; done
  # #4845 — every Claude role home carries its MCP config, as the repo's do
  for r in wren kade silas; do printf '{"mcpServers":{"chorus-api":{"command":"chorus-runner","args":["stdio"]}}}\n' > "$T/roles/$r/.mcp.json"; done
  printf '' > "$T/spine-read.log"
}

login_harness_env() {
  export CLAUDE_BIN="$T/bin/claude" TMUX_BIN="$T/bin/tmux" AWAKE_PS="$T/bin/ps"
  export CHORUS_TOKEN_BIN="$T/bin/token" AWAKE_CURL="$T/bin/curl" CHORUS_LOG_BIN="$T/bin/chorus-log"
  export AWAKE_PROBE_BIN="$T/bin/probe" AWAKE_OSASCRIPT="$T/bin/osascript" AWAKE_OPEN="$T/bin/open"
  export AWAKE_SERVICES="identity=http://stub:3001/,chorus-api=http://stub:3340/h,athena-make=http://stub:3360/s"
  export AWAKE_HOOKS_SOCKET=none   # #4409: the hooks check is off unless a test points it somewhere
  export AWAKE_RELAY_PROJECT="$T/bin/relay"   # #4445: never the real relay from a test
  export AWAKE_SERVICE_WAIT=2 AWAKE_NO_RETRY=1
  export CHORUS_IDENTITY_DIR="$T/identity" CHORUS_API_URL="http://stub:3360"
  export CHORUS_SESSIONS_DIR="$T/sessions" AWAKE_ROLES_BASE="$T/roles" CHORUS_ROOT="$ROOT"
  # #4933 — the stub pane runs the wrapper's establish with the binary under test
  # (read only by the stub tmux; the launch line itself names the installed path)
  export LOGIN_HARNESS_PRINCIPAL="${PRINCIPAL:-${SCRIPT:-${PRINCIPAL_BIN:-}}}"
  export AWAKE_PROJECTS_DIR="$T/projects" AWAKE_NO_ATTACH=1 AWAKE_WAIT=2 USER=unit-account
  export AWAKE_VSCODE_DIR="$T/vscode" CHORUS_PRINCIPAL_BIN="/h/.chorus/bin/chorus-principal"
  export AWAKE_SUDO="$T/bin/sudo" AWAKE_ACCOUNT_HOMES="$T/homes"
  export AWAKE_LAUNCHCTL="$T/bin/launchctl"
  export CHORUS_LOG_FILE="$T/spine-read.log"
  unset TMUX CLAUDECODE CHORUS_ROLE AWAKE_ROLE_DIR
}

login_harness() { login_harness_stubs; login_harness_env; }

# #4933 — EST: the role's Principal names the account running the test, so the role's
# own establish (run by its wrapper, or by a case directly) is that account's to run
est() { printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$(id -un)" > "$T/principal-$1.json"; }

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
body() { cat "$T"/bodies/*-"$1"-"$2".json 2>/dev/null | tail -1; }   # last body sent: METHOD route
bodies() { ls "$T/bodies" | sed 's/^[0-9]*-//'; }
has() { printf '%s' "$1" | grep -qF -- "$2"; }
# the name a row file holds: row_name <role> <session|run|presence>
row_name() { python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/$1/$2.row.json"; }
