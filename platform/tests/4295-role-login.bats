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
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  PRINCIPAL="$SCRIPT"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  # #4409 — the shared fixture world (lib/login-harness.bash), not a copy of it
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}
# the last POST/PUT body login sent (the harness keeps every one under bodies/)
curlbody() { cat "$(ls "$T"/bodies/*.json | tail -1)"; }

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
  out_has "chorus-principal logout kade"
}

# ------------------------------------------------------------------ off

@test "off: stops the role and closes its login row" {
  running silas 901
  printf '{"state":"recorded","session":"silas-x-1","pid":901}' > "$T/identity/silas/login.json"
  printf '{"name":"silas-x-1","ownedBy":"principal-silas","tokenId":"t1","sessionState":"open","endedAt":""}' > "$T/identity/silas/session.row.json"
  run "$SCRIPT" off silas
  test "$status" -eq 0
  out_has "silas logged out: login closed (session silas-x-1)"
  grep -q -- "-X PUT" "$T/curl.log"
  curlbody | grep -q '"sessionState":"closed"'
  # the PUT replaces the whole row: the owner and token must ride along
  curlbody | grep -q '"ownedBy":"principal-silas"'
  curlbody | grep -q '"tokenId":"t1"'
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

@test "/exit ends the run, not the login (#4406); the pane is not killed (it is already ending)" {
  running silas 903
  printf '{"state":"recorded","session":"silas-x-3","pid":903}' > "$T/identity/silas/login.json"
  printf '{"data":[{"name":"silas-x-3","ownedBy":"principal-silas","tokenId":"t3","sessionState":"open"}]}' > "$T/row.json"
  run bash -c "echo '{\"reason\":\"prompt_input_exit\"}' | CLAUDECODE=1 CHORUS_ROLE=silas '$SCRIPT' off silas --from-exit"
  test "$status" -eq 0
  state_is silas recorded
  grep -q "session.run.exited silas" "$T/spine.log"
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
  grep -q '"command": "/h/.chorus/bin/chorus-principal login wren"' "$T/vscode/tasks.json"
  grep -q '"runOn": "folderOpen"' "$T/vscode/tasks.json"
  grep -q "open -a Visual Studio Code" "$T/open.log"
  grep -q 'do script "/h/.chorus/bin/chorus-principal login silas"' "$T/osa.log"
  grep -q 'do script "/h/.chorus/bin/chorus-principal login kade"' "$T/osa.log"
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
  test -z "$(grep -F 'login silas' "$T/osa.log" || true)"
  grep -q 'login kade' "$T/osa.log"
}

@test "up --windows leaves an existing tasks.json alone" {
  echo '{"version":"2.0.0","tasks":[]}' > "$T/vscode/tasks.json"
  run "$SCRIPT" up --windows
  grep -qx '{"version":"2.0.0","tasks":\[\]}' "$T/vscode/tasks.json"
  out_has "exists without the wren task"
}

# ------------------------------------------------------------------ chorus-principal

@test "chorus-principal login|logout|status are the verbs (Jeff types chorus-principal)" {
  [ -x "$PRINCIPAL" ] || skip "chorus-principal not built at $PRINCIPAL"
  run "$PRINCIPAL" status
  test "$status" -eq 0
  out_has "silas  off"
  run "$PRINCIPAL" login silas
  test "$status" -eq 0
  out_has "logged in  via claude -c"
  run "$PRINCIPAL" logout silas
  test "$status" -eq 0
  out_has "silas logged out"
}

@test "#4345 on and off stay as aliases for login and logout" {
  [ -x "$PRINCIPAL" ] || skip "chorus-principal not built at $PRINCIPAL"
  run "$PRINCIPAL" on silas
  test "$status" -eq 0
  out_has "logged in  via claude -c"
  run "$PRINCIPAL" off silas
  test "$status" -eq 0
  out_has "silas logged out"
}

@test "NEGATIVE PROOF: an unknown verb is refused, not read as a login" {
  [ -x "$PRINCIPAL" ] || skip "chorus-principal not built at $PRINCIPAL"
  run "$PRINCIPAL" logon silas
  test "$status" -eq 2
}
