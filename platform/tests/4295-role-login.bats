#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake and chorus-principal binaries with stub tmux, claude, ps, token-minter, curl, service probe, osascript and open; no live services, no live panes.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4295 — Jeff, 2026-09-25: "we wanted chorus-principal on|off to start and
# exit"; "i dont want a swat on every reboot"; "if u can automate all of that
# that is very helpful for me my bar for friction here is low"; "a negative
# signin experience is a problem". The 09:44 reboot that morning: two roles
# came up with no login, "already awake ... registered yes" was printed over
# both, and Kade's session started Silas.
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
  # stub curl: POST/PUT answer the fixture status; GET answers the row fixture
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
echo "curl \$*" >> "$T/curl.log"
case "\$*" in
  *"-X POST"*|*"-X PUT"*) for a in "\$@"; do case "\$a" in @*session.body) cp "\${a#@}" "$T/curl.body" ;; esac; done; cat "$T/curl.status" 2>/dev/null || echo 201 ;;
  *) cat "$T/row.json" 2>/dev/null ;;
esac
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

# ------------------------------------------------------------------ on

@test "on: a stopped role starts logged in, and the line says logged in" {
  run "$SCRIPT" on silas
  test "$status" -eq 0
  out_has "login: silas"
  out_has "recorded yes"
  out_has "logged in  via claude -c"
  out_lacks "registered yes"
  state_is silas recorded
  grep -q "set-option -t chorus-silas status-right  logged in |" "$T/tmux.log"
  # the full row is kept so `off` can close it
  grep -q '"ownedBy":"principal-silas"' "$T/identity/silas/session.row.json"
}

@test "on: a RUNNING role with no login is logged in, not blessed (the 09:45 case)" {
  running kade 4264
  run "$SCRIPT" on kade
  test "$status" -eq 0
  out_has "kade is running without a login; logging it in now"
  grep -q "token kade" "$T/token.log"
  grep -q -- "-X POST" "$T/curl.log"
  out_has "pane %0  logged in  via already awake"
  out_lacks "registered yes"
  grep -q '"pid":4264' "$T/identity/kade/login.json"
  # never a second copy
  test -z "$(grep -F send-keys "$T/tmux.log" 2>/dev/null || true)"
}

@test "NEGATIVE PROOF: a login recorded for an EARLIER pid does not count for this one" {
  running kade 46809
  printf '{"state":"recorded","session":"kade-old-1","pid":4264}' > "$T/identity/kade/login.json"
  run "$SCRIPT" on kade
  out_has "kade is running without a login"
  grep -q "token kade" "$T/token.log"
  grep -q '"pid":46809' "$T/identity/kade/login.json"
}

@test "on: a running role whose login is on file for this pid is left alone" {
  running kade 4264
  printf '{"state":"recorded","session":"kade-x-1","pid":4264}' > "$T/identity/kade/login.json"
  run "$SCRIPT" on kade
  test "$status" -eq 0
  test ! -f "$T/token.log"
  out_has "logged in  via already awake"
}

@test "on: a service still down counts down, names it, and the role STARTS with the login pending" {
  touch "$T/down-3001"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  out_has "waiting for identity :3001"
  out_has "login: silas  pending — identity not answering after 0:02"
  out_has "logs itself in when that answers; nothing for you to do"
  out_has "login pending  via claude -c"
  out_lacks "UNAUTHENTICATED"
  state_is silas pending
  grep -q "send-keys -t chorus-silas" "$T/tmux.log"
  grep -q "status-right  login pending |" "$T/tmux.log"
}

@test "relogin: once the service answers, the pending login records itself" {
  touch "$T/down-3001"
  run "$SCRIPT" on silas
  state_is silas pending
  rm "$T/down-3001"
  run env AWAKE_RETRY_EVERY=1 AWAKE_RETRY_SECS=5 "$SCRIPT" relogin silas
  test "$status" -eq 0
  state_is silas recorded
  grep -q "^session.login.recovered silas " "$T/spine.log"
  grep -q "status-right  logged in |" "$T/tmux.log"
}

@test "relogin gives up at its bound and says so on the spine" {
  touch "$T/down-3001"
  run "$SCRIPT" on silas
  run env AWAKE_RETRY_EVERY=1 AWAKE_RETRY_SECS=1 "$SCRIPT" relogin silas
  test "$status" -eq 1
  grep -q "^session.login.gave_up silas " "$T/spine.log"
}

@test "a pending login starts the background retry by itself" {
  touch "$T/down-3001"
  run env AWAKE_NO_RETRY=0 AWAKE_RETRY_EVERY=1 AWAKE_RETRY_SECS=6 "$SCRIPT" on silas
  state_is silas pending
  rm "$T/down-3001"
  for i in 1 2 3 4 5 6 7 8; do grep -q '"state":"recorded"' "$T/identity/silas/login.json" && break; sleep 1; done
  state_is silas recorded
}

@test "a role session cannot start another role (Kade started Silas, 09:50)" {
  run env CLAUDECODE=1 CHORUS_ROLE=kade "$SCRIPT" on silas
  test "$status" -eq 2
  out_has "a kade session cannot start silas"
  test ! -f "$T/tmux.log"
  test ! -f "$T/token.log"
}

@test "NEGATIVE PROOF: the same command from Jeff's shell in roles/kade is allowed" {
  run env CHORUS_ROLE=kade "$SCRIPT" on silas
  test "$status" -eq 0
  grep -q "send-keys -t chorus-silas" "$T/tmux.log"
}

@test "two live sessions for one role: refused, and the fix is named" {
  running kade 11; running kade 12
  run "$SCRIPT" on kade
  test "$status" -eq 1
  out_has "chorus-principal off kade"
}

# ------------------------------------------------------------------ off

@test "off: stops the role and closes its login row" {
  running silas 901
  printf '{"state":"recorded","session":"silas-x-1","pid":901}' > "$T/identity/silas/login.json"
  printf '{"name":"silas-x-1","ownedBy":"principal-silas","tokenId":"t1","sessionState":"open","endedAt":""}' > "$T/identity/silas/session.row.json"
  run "$SCRIPT" off silas
  test "$status" -eq 0
  out_has "silas off: login closed (session silas-x-1)"
  grep -q -- "-X PUT" "$T/curl.log"
  grep -q '"sessionState":"closed"' "$T/curl.body"
  # the PUT replaces the whole row: the owner and token must ride along
  grep -q '"ownedBy":"principal-silas"' "$T/curl.body"
  grep -q '"tokenId":"t1"' "$T/curl.body"
  grep -q "kill-session -t chorus-silas" "$T/tmux.log"
  state_is silas closed
  grep -q "^session.logout silas session=silas-x-1 how=off" "$T/spine.log"
  test ! -f "$T/sessions/silas-901.json"
}

@test "off: a row that cannot be closed still stops the role and says the row expires" {
  running silas 902
  printf '{"state":"recorded","session":"silas-x-2","pid":902}' > "$T/identity/silas/login.json"
  printf '{"data":[{"name":"someone-else","ownedBy":"principal-kade","tokenId":"k"}]}' > "$T/row.json"
  run "$SCRIPT" off silas
  test "$status" -eq 0
  out_has "could not be closed"
  grep -q "kill-session -t chorus-silas" "$T/tmux.log"
}

@test "/exit logs out; the pane is not killed (it is already ending)" {
  running silas 903
  printf '{"state":"recorded","session":"silas-x-3","pid":903}' > "$T/identity/silas/login.json"
  printf '{"data":[{"name":"silas-x-3","ownedBy":"principal-silas","tokenId":"t3","sessionState":"open"}]}' > "$T/row.json"
  run bash -c "echo '{\"reason\":\"prompt_input_exit\"}' | CLAUDECODE=1 CHORUS_ROLE=silas '$SCRIPT' off silas --from-exit"
  test "$status" -eq 0
  state_is silas closed
  grep -q "how=exit" "$T/spine.log"
  test -z "$(grep -F kill-session "$T/tmux.log" 2>/dev/null || true)"
}

@test "NEGATIVE PROOF: /clear is not a logout" {
  running silas 904
  printf '{"state":"recorded","session":"silas-x-4","pid":904}' > "$T/identity/silas/login.json"
  run bash -c "echo '{\"reason\":\"clear\"}' | CLAUDECODE=1 CHORUS_ROLE=silas '$SCRIPT' off silas --from-exit"
  test "$status" -eq 0
  state_is silas recorded
  test ! -f "$T/curl.log"
}

@test "a role session cannot stop another role" {
  running silas 905
  run env CLAUDECODE=1 CHORUS_ROLE=kade "$SCRIPT" off silas
  test "$status" -eq 2
  out_has "a kade session cannot stop silas"
  test ! -f "$T/tmux.log"
}

# ------------------------------------------------------------------ status

@test "status: one line per role, and never 'registered yes'" {
  running wren 5499
  printf '{"state":"recorded","session":"wren-x","pid":5499}' > "$T/identity/wren/login.json"
  running kade 4264
  printf '{"state":"pending","why":"identity not answering","pid":4264}' > "$T/identity/kade/login.json"
  run "$SCRIPT" status
  test "$status" -eq 0
  out_has "wren   running  logged in      answering"
  out_has "kade   running  login pending  answering   (identity not answering, retrying)"
  out_has "silas  off"
  out_lacks "registered yes"
}

# ------------------------------------------------------------------ up

@test "up: all three start logged in and Jeff gets ONE line" {
  run "$SCRIPT" up
  test "$status" -eq 0
  out_has "wren kade silas up, 3 of 3 logged in"
  grep -q 'display notification "wren kade silas up, 3 of 3 logged in" with title "Chorus"' "$T/osa.log"
  grep -q "^roles.up system summary=wren kade silas up, 3 of 3 logged in" "$T/spine.log"
}

@test "up with identity down: all three still start, the line says pending, nothing for Jeff to do" {
  touch "$T/down-3001"
  run "$SCRIPT" up
  test "$status" -eq 0
  out_has "wren kade silas up, 0 of 3 logged in"
  out_has "kade login pending (identity not answering"
  out_has "retrying on its own"
  for r in wren kade silas; do grep -q "send-keys -t chorus-$r" "$T/tmux.log"; done
}

@test "up --windows: VS Code gets the Wren task, Terminal opens Silas and Kade" {
  run "$SCRIPT" up --windows
  test "$status" -eq 0
  grep -q '"command": "/h/.chorus/bin/chorus-principal on wren"' "$T/vscode/tasks.json"
  grep -q '"runOn": "folderOpen"' "$T/vscode/tasks.json"
  grep -q "open -a Visual Studio Code" "$T/open.log"
  grep -q 'do script "/h/.chorus/bin/chorus-principal on silas"' "$T/osa.log"
  grep -q 'do script "/h/.chorus/bin/chorus-principal on kade"' "$T/osa.log"
}

@test "up --windows: VS Code is told to run the task without asking, and its other settings stay" {
  printf '{\n  "rust-analyzer.linkedProjects": ["a/Cargo.toml"]\n}\n' > "$T/vscode/settings.json"
  run "$SCRIPT" up --windows
  grep -q '"task.allowAutomaticTasks": "on"' "$T/vscode/settings.json"
  grep -q '"rust-analyzer.linkedProjects"' "$T/vscode/settings.json"
}

@test "up --windows: a role that already has a window gets no second one" {
  echo "/dev/ttys005: chorus-silas" > "$T/clients-chorus-silas"
  run "$SCRIPT" up --windows
  test -z "$(grep -F 'on silas' "$T/osa.log" || true)"
  grep -q 'on kade' "$T/osa.log"
}

@test "up --windows leaves an existing tasks.json alone" {
  echo '{"version":"2.0.0","tasks":[]}' > "$T/vscode/tasks.json"
  run "$SCRIPT" up --windows
  grep -qx '{"version":"2.0.0","tasks":\[\]}' "$T/vscode/tasks.json"
  out_has "exists without the wren task"
}

# ------------------------------------------------------------------ chorus-principal

@test "chorus-principal on|off|status are the same verbs (Jeff types chorus-principal)" {
  [ -x "$PRINCIPAL" ] || skip "chorus-principal not built at $PRINCIPAL"
  run "$PRINCIPAL" status
  test "$status" -eq 0
  out_has "silas  off"
  run "$PRINCIPAL" on silas
  test "$status" -eq 0
  out_has "logged in  via claude -c"
}
