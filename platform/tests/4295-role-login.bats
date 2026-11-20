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
# #4414 (AC1): a role running outside tmux is moved into its pane by one login, never blessed "already awake"

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/target/$(id -un)/release/chorus-principal}"
  PRINCIPAL="$SCRIPT"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  # #4409 — the shared fixture world (lib/login-harness.bash), not a copy of it
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # #4933 — each role's own wrapper establishes its session (as the account running
  # the test); the launcher writes nothing, so every row below is the role's own
  for r in silas kade wren; do est "$r"; done
}
# the last POST/PUT body login sent (the harness keeps every one under bodies/)
curlbody() { cat "$(ls "$T"/bodies/*.json | tail -1)"; }

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }
state_is() { grep -q "\"state\":\"$2\"" "$T/identity/$1/login.json"; }

# ------------------------------------------------------------------ on

@test "on: a stopped role starts logged in, and the line says logged in" {
  : > "$T/curl.log"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  # #4933 — the role's own establish (in its pane) signs in and records; the launcher only reads it back
  grep -q "login: silas" "$T/establish.log"
  grep -q "recorded yes" "$T/establish.log"
  out_has "logged in  via claude -c"
  out_lacks "registered yes"
  state_is silas recorded
  grep -q "set-option -t chorus-silas status-right  logged in |" "$T/tmux.log"
  # the full row is kept so `off` can close it
  grep -q '"ownedBy":"principal-silas"' "$T/identity/silas/session.row.json"
}

@test "on: a RUNNING role with no login is logged in, not blessed (the 09:45 case)" {
  running kade 4264
  : > "$T/token.log"; : > "$T/curl.log"
  run "$SCRIPT" on kade
  test "$status" -eq 0
  # #4933 (FLIP) — never logged in from the launcher: the role establishes on its next turn
  out_has "kade is running without a session; it establishes its own on its next turn"
  test -z "$(grep "token kade" "$T/token.log" || true)"
  test -z "$(grep -E -- '-X (POST|PUT|PATCH)' "$T/curl.log" || true)"
  out_has "pane %0  login pending  via already awake"
  out_lacks "registered yes"
  grep -q '"pid":4264' "$T/identity/kade/login.json"
  # never a second copy
  test -z "$(grep -E 'respawn-pane|send-keys' "$T/tmux.log" 2>/dev/null || true)"
}

@test "NEGATIVE PROOF: a login recorded for an EARLIER pid does not count for this one" {
  running kade 46809
  printf '{"state":"recorded","session":"kade-old-1","pid":4264}' > "$T/identity/kade/login.json"
  : > "$T/token.log"
  run "$SCRIPT" on kade
  # the login on file is for pid 4264, so pid 46809 is NOT logged in by it (#4933: and not by the launcher)
  out_has "kade is running without a session"
  test -z "$(grep "token kade" "$T/token.log" || true)"
  grep -q '"pid":46809' "$T/identity/kade/login.json"
  state_is kade pending
}

# #4414 AC1 — Jeff 09-30 15:48: login found a Silas in a plain window (ttys007, host unknown,
# no tmux pane), said "already awake", and started nothing in tmux; nudges could not reach it.
stray() {
  printf '{"role":"%s","pid":%s,"tty":"/dev/ttys007","host":"unknown","tmux":""}' "$1" "$2" > "$T/sessions/$1-$2.json"
  echo "$2" >> "$T/alive-pids"; echo "$2" >> "$T/no-tmux-pids"   # a plain window: no tmux ancestor
  printf '{"role":"%s","event":"reply.published","timestamp":"%s"}\n' "$1" "$(date '+%Y-%m-%dT%H:%M:%S')" >> "$T/spine-read.log"
}

@test "#4414 AC1: a role running outside tmux is moved into its tmux pane, and the line says so" {
  # the stray's own establish opened a run (as the role does on its first turn)
  "$SCRIPT" establish silas --run 1a2b >/dev/null 2>&1
  stray silas 10263
  printf '{"state":"recorded","session":"silas-x-1","pid":10263}' > "$T/identity/silas/login.json"
  : > "$T/claude.log"; : > "$T/tmux.log"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  out_has "moving silas into tmux"
  grep -q "claude stop 10263" "$T/claude.log"
  test ! -f "$T/sessions/silas-10263.json"
  grep -q "new-session" "$T/tmux.log"
  # Silas 14:12 — the stray's open run row is closed (the start ends it as a restart), not left for the sweep to call a crash
  r=$(cat "$T"/bodies/*PATCH-identity_sessionruns_* 2>/dev/null | tail -1); printf '%s' "$r" | grep -q '"endReason":"restart"'
  # and the role resumes its conversation in the pane, not a fresh one
  out_has "via claude -c"
}

@test "#4414 AC1 NEGATIVE PROOF: a role outside tmux is never blessed already awake" {
  stray silas 10263
  printf '{"state":"recorded","session":"silas-x-1","pid":10263}' > "$T/identity/silas/login.json"
  run "$SCRIPT" on silas
  out_lacks "via already awake"
}

@test "#4414 (review) NEGATIVE PROOF: a stray that will not stop is refused, never a second session" {
  stray silas 10263
  touch "$T/stop-fails"
  : > "$T/tmux.log"
  run env AWAKE_STOP_WAIT=1 "$SCRIPT" on silas
  test "$status" -ne 0
  out_has "could not end silas's session outside tmux"
  test -f "$T/sessions/silas-10263.json"
  test -z "$(grep -E 'respawn-pane|new-session' "$T/tmux.log" || true)"
}

@test "#4414 (review) NEGATIVE PROOF: a start that is refused leaves the stray running, never no session" {
  stray silas 10263
  rm -f "$T/roles/silas/.mcp.json"
  : > "$T/claude.log"
  run "$SCRIPT" on silas
  test "$status" -ne 0
  test -z "$(grep "claude stop 10263" "$T/claude.log" || true)"
  test -f "$T/sessions/silas-10263.json"
}

@test "#4414 (review) NEGATIVE PROOF: a session with a named pane (%5) is never judged outside, whatever its tty" {
  printf '{"role":"silas","pid":10263,"tty":"","host":"tmux","tmux":"%%5"}' > "$T/sessions/silas-10263.json"
  echo 10263 >> "$T/alive-pids"
  # a talking session (as `running` writes), so only the pane rule is under test, not the mute rule
  printf '{"role":"silas","event":"reply.published","timestamp":"%s"}\n' "$(date '+%Y-%m-%dT%H:%M:%S')" >> "$T/spine-read.log"
  printf '{"state":"recorded","session":"silas-x-1","pid":10263}' > "$T/identity/silas/login.json"
  : > "$T/claude.log"
  run "$SCRIPT" on silas
  out_lacks "moving silas into tmux"
  test -z "$(grep "claude stop 10263" "$T/claude.log" || true)"
}

@test "#4414 AC1 NEGATIVE PROOF: a role registered without a pane but running under tmux is left alone (the live case)" {
  printf '{"role":"silas","pid":10263,"tty":"/dev/ttys015","host":"unknown"}' > "$T/sessions/silas-10263.json"
  echo 10263 >> "$T/alive-pids"
  printf '{"role":"silas","event":"reply.published","timestamp":"%s"}\n' "$(date '+%Y-%m-%dT%H:%M:%S')" >> "$T/spine-read.log"
  printf '{"state":"recorded","session":"silas-x-1","pid":10263}' > "$T/identity/silas/login.json"
  : > "$T/claude.log"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  out_lacks "moving silas into tmux"
  out_has "via already awake"
  test -z "$(grep "claude stop 10263" "$T/claude.log" || true)"
}

@test "#4414 AC1 NEGATIVE PROOF: a process table that cannot be read ends nothing, and says it cannot tell" {
  stray silas 10263
  touch "$T/ps-unreadable"
  printf '{"state":"recorded","session":"silas-x-1","pid":10263}' > "$T/identity/silas/login.json"
  : > "$T/claude.log"
  run "$SCRIPT" on silas
  out_has "cannot tell if silas is in tmux"
  test -z "$(grep "claude stop 10263" "$T/claude.log" || true)"
}

@test "#4414 (review 2): a stray that takes a moment to exit is waited for, not refused" {
  stray silas 10263
  touch "$T/stop-slow"
  run env AWAKE_STOP_WAIT=3 "$SCRIPT" on silas
  test "$status" -eq 0
  out_has "moving silas into tmux"
  out_lacks "could not end"
}

@test "#4414 (review 2): login from inside the role's own pane still ends the stray first" {
  stray silas 10263
  echo chorus-silas > "$T/own-session"
  : > "$T/claude.log"
  run env TMUX=1 AWAKE_NO_ATTACH=0 "$SCRIPT" on silas
  grep -q "claude stop 10263" "$T/claude.log"
  test ! -f "$T/sessions/silas-10263.json"
}

@test "on: a running role whose login is on file for this pid is left alone" {
  running kade 4264
  printf '{"state":"recorded","session":"kade-x-1","pid":4264}' > "$T/identity/kade/login.json"
  run "$SCRIPT" on kade
  test "$status" -eq 0
  test ! -f "$T/token.log"
  out_has "logged in  via already awake"
}

@test "on: identity still down counts down, names it, and the role does NOT start (Jeff 11-08: a failed sign-in does not start)" {
  touch "$T/down-3001"
  run "$SCRIPT" on silas
  out_has "waiting for identity :3001"
  grep -q "respawn-pane -k -t chorus-silas" "$T/tmux.log"
  # the role's own establish could not sign in, so its runtime never started
  grep -q "identity not answering" "$T/establish.log"
  out_has "registered NO"
  out_has "identity not answering after 0:02"
  out_lacks "UNAUTHENTICATED"
  test "$status" -ne 0
}

# #4933 RETIRED (three cases): relogin's background retry ("once the service answers
# the pending login records itself", "gives up at its bound", "a pending login starts
# the retry by itself"). The launcher no longer signs in or retries; relogin exits 2
# and names its replacement (retirement gate: launch_hop_writes_nothing_4933.bats).
# Recovery is the role's own next turn, below.
@test "#4933 recovery is the role's next turn: a turn with no session establishes it, as the role" {
  est silas
  : > "$T/curl.log"
  echo '{"prompt":"hi"}' | AWAKE_SEEN_SYNC=1 AWAKE_SEEN_EVERY=0 "$SCRIPT" seen silas
  state_is silas recorded
  grep -qE -- '-X POST.*/v1/identity/logins' "$T/curl.log"
}

@test "#4933 NEGATIVE PROOF: a turn by an account that is not the role's establishes nothing" {
  # fixture role fx's row names chorus-fx, not the account running this
  printf '{"data":{"principalKind":"agent","hostAccount":"chorus-fx"}}\n200\n' > "$T/principal-fx.json"
  mk_token fx; mkdir -p "$T/identity/fx"
  : > "$T/curl.log"
  echo '{"prompt":"hi"}' | AWAKE_SEEN_SYNC=1 AWAKE_SEEN_EVERY=0 AWAKE_ESTABLISH_EVERY=0 "$SCRIPT" seen fx
  test -z "$(grep -E -- '-X POST.*/v1/identity/logins' "$T/curl.log" || true)"
  # and the same turn with fx's row naming this account does establish (the pair differs)
  printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$(id -un)" > "$T/principal-fx.json"
  echo '{"prompt":"hi"}' | AWAKE_SEEN_SYNC=1 AWAKE_SEEN_EVERY=0 AWAKE_ESTABLISH_EVERY=0 "$SCRIPT" seen fx
  grep -qE -- '-X POST.*/v1/identity/logins' "$T/curl.log"
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
  grep -q "respawn-pane -k -t chorus-silas" "$T/tmux.log"
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
  # #4517 AC6 — the owner asks the door to end it, naming only the session; the door stamps
  # how it ended, and the role writes nothing to the Session row itself
  grep -qE -- "-X POST.*/v1/identity/logouts" "$T/curl.log" || return 1
  test -z "$(grep -E -- "-X (PATCH|PUT).*/v1/identity/sessions" "$T/curl.log" || true)" || return 1
  grep -q '"session":"silas-x-1"' "$T/identity/silas/logout.body" || return 1
  test -z "$(grep -F -e '"ownedBy"' -e '"endedAt"' -e '"sessionState"' "$T/identity/silas/logout.body" || true)" || return 1
  curlbody | grep -q '"sessionState":"closed"' || return 1
  grep -q "kill-session -t chorus-silas" "$T/tmux.log"
  state_is silas closed
  grep -q "^session.logout silas session=silas-x-1 how=off" "$T/spine.log"
  test ! -f "$T/sessions/silas-901.json"
}

# #4950 — since #4944 the close is a PATCH that needs no row read back, so "cannot be closed"
# is the door refusing it; and since #4948 nothing expires a session, so it no longer says so
@test "off: a row the door refuses to close still stops the role and says what closes it" {
  running silas 902
  printf '{"state":"recorded","session":"silas-x-2","pid":902}' > "$T/identity/silas/login.json"
  echo 403 > "$T/curl.status.identity_logouts"
  run "$SCRIPT" off silas
  test "$status" -eq 0
  out_has "could not be closed"
  out_has "until the role's next login supersedes it"
  out_lacks "expires on its own"
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
  printf '{"name":"wren-x"}' > "$T/identity/wren/session.row.json"
  printf '{"data":[{"name":"wren-x","ownedBy":"principal-wren"}]}' > "$T/open-sessions.json"
  running kade 4264
  printf '{"state":"pending","why":"identity not answering","pid":4264}' > "$T/identity/kade/login.json"
  run "$SCRIPT" status
  test "$status" -eq 0
  out_has "wren   running  logged in      answering"
  out_has "kade   running  login pending  answering   (identity not answering, retrying)"
  out_has "silas  off"
  out_lacks "registered yes"
}

@test "#4997 NEGATIVE PROOF: a recorded login whose row is not open reads NOT logged in" {
  running wren 5499
  printf '{"state":"recorded","session":"wren-x","pid":5499}' > "$T/identity/wren/login.json"
  printf '{"name":"wren-x"}' > "$T/identity/wren/session.row.json"
  printf '{"data":[]}' > "$T/open-sessions.json"
  run "$SCRIPT" status
  out_has "wren   running  NOT logged in"
  out_has "(session row closed)"
  test -z "$(printf '%s' "$output" | grep -E '^wren +running +logged in' || true)"
}

# ------------------------------------------------------------------ up

@test "up: all three start logged in and Jeff gets ONE line" {
  # #4845 — up starts every agent the roles door lists; this world lists the three Claude roles
  printf '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"}]}\n' > "$T/roles-door.json"
  run "$SCRIPT" up
  test "$status" -eq 0
  out_has "kade silas wren up, 3 of 3 logged in"
  grep -q 'display notification "kade silas wren up, 3 of 3 logged in" with title "Chorus"' "$T/osa.log"
  grep -q "^roles.up system summary=kade silas wren up, 3 of 3 logged in" "$T/spine.log"
}

@test "up with identity down: no role starts (Jeff 11-08), and the line says why" {
  # #4845 — up starts every agent the roles door lists; this world lists the three Claude roles
  printf '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"}]}\n' > "$T/roles-door.json"
  touch "$T/down-3001"
  run "$SCRIPT" up
  # #4933 — each pane was launched, each role's own sign-in failed, so none started
  for r in wren kade silas; do grep -q "respawn-pane -k -t chorus-$r" "$T/tmux.log"; done
  test "$(grep -c "identity not answering" "$T/establish.log")" -ge 3
  out_has "no role started"
  for r in kade silas wren; do out_has "$r  not started: its sign-in could not happen — identity not answering"; done
}

@test "up --windows: VS Code gets the Wren task, Terminal opens Silas and Kade" {
  # #4845 — up starts every agent the roles door lists; this world lists the three Claude roles
  printf '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"}]}\n' > "$T/roles-door.json"
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
