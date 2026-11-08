#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake binary with stub claude, tmux, ps, token-minter, curl and chorus-log; no live services.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4202 — Jeff, 2026-09-17: "to me chorus-awake must include authn for agents";
# "agents must login to chorus"; "and then follow authz rules". Before a role's
# first turn chorus-awake obtains the role's token, checks it names that role's
# WebID, records a Session row through the security API, emits session.login,
# and hands the pane the token file. No token → nothing starts.
#
# Asserts are simple commands, never `[[ ]]`: on bash 3.2 a failing `[[` that is
# not the last line of a test passes it (91 hollow suites found 2026-09-16).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  # #4409 — the shared fixture world (lib/login-harness.bash); only what this
  # suite needs differently is set below.
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # one role, one window: has-session answers from a marker; registration is
  # simulated by the cases themselves, so the launch (respawn-pane) registers nothing
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
case "\$1" in has-session) [ -f "$T/tmux-session-exists" ]; exit \$? ;; new-session) touch "$T/tmux-session-exists" ;; esac
# #4432 — the session registers when the pane is launched (respawn-pane since
# #4813; was send-keys), not
# after a fixed sleep: a 0.3 s timer raced the login's own steps and failed
# run 15 on unchanged code.
if [[ " \$* " == *" respawn-pane "* ]] && [ -f "$T/reg-on-send" ]; then
  read -r pid pane < "$T/reg-on-send"
  printf '{"role":"kade","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%s"}' "\$pid" "\${pid: -1}" "\$pane" > "$T/sessions/kade-\$pid.json"
  echo "\$pid" >> "$T/alive-pids"
fi
exit 0
EOS
  chmod +x "$T/bin/tmux"
  export AWAKE_ROLE_DIR="$T/roles/kade" AWAKE_WAIT=1 CHORUS_API_URL="http://stub:1"
  # #4295: no live service probes from a unit world; no background retry left running
  export AWAKE_SERVICES=none AWAKE_NO_RETRY=1
  one_token kade
}

# a JWT-shaped token whose payload names <role>'s WebID; the signature is not checked here (CSS signs, the API verifies)
# the one token every role is handed in this suite (token.fixture); named apart
# from the harness's per-role mk_token, which sourcing the harness would replace
one_token() {
  local role="$1" exp="${2:-$(( $(date +%s) + 600 ))}"
  local payload
  payload=$(printf '{"webid":"https://id.lightlifeurbangardens.com/%s/profile/card#me","jti":"jti-%s-0001","iat":%s,"exp":%s}' "$role" "$role" "$(date +%s)" "$exp" | base64 | tr '+/' '-_' | tr -d '=\n')
  printf 'eyJhbGciOiJFUzI1NiJ9.%s.sig' "$payload" > "$T/token.fixture"
}

reg_on_launch() { echo "$1 ${2:-%0}" > "$T/reg-on-send"; }
reg() { printf '{"role":"kade","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%s"}' "$1" "${1: -1}" "${2:-%0}" > "$T/sessions/kade-$1.json"; echo "$1" >> "$T/alive-pids"; }
out_has() { printf '%s' "$output" | grep -q -- "$1"; }
file_has() { grep -q -- "$2" "$1"; }
file_lacks() { ! grep -q -- "$2" "$1"; }

@test "the launcher signs in as nobody: no token asked, no row written; the pane is still launched" {
  # #4933 (FLIP): the role's own wrapper signs in (establish), never the launcher
  reg_on_launch 777 %5
  : > "$T/token.log"; : > "$T/curl.log"
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  lacks "$T/token.log" "token kade"
  test -z "$(grep -E -- '-X (POST|PUT)' "$T/curl.log" || true)"
  file_has "$T/tmux.log" "respawn-pane -k -t chorus-kade"
  # the pane gets the token FILE, never the token on the command line
  file_has "$T/tmux.log" "CHORUS_SESSION_TOKEN_FILE='$T/homes/chorus-kade/.chorus/identity/kade/token.cache'"
  file_lacks "$T/tmux.log" "eyJhbGciOiJFUzI1NiJ9"
}

@test "the login is recorded as a Session row through the security API, owned by the principal" {
  # #4933 (MOVE): the role's own establish records it, as the role's account
  est kade
  run "$SCRIPT" establish kade
  [ "$status" -eq 0 ]
  file_has "$T/curl.log" "http://stub:1/v1/identity/sessions"
  file_has "$T/curl.body" '"ownedBy":"principal-kade"'
  file_has "$T/curl.body" '"tokenId":"jti-kade-0001"'
  file_has "$T/curl.body" '"sessionState":"open"'
  file_has "$T/curl.body" "\"hostAccount\":\"$(id -un)\""
  # the token travels as a header FILE, not as a curl argument
  file_lacks "$T/curl.log" "eyJhbGciOiJFUzI1NiJ9"
  out_has "login: kade"
  out_has "recorded yes"
}

@test "session.login is on the spine with the WebID and the token id, never the token" {
  est kade
  run "$SCRIPT" establish kade
  [ "$status" -eq 0 ]
  file_has "$T/spine.log" "^session.login kade "
  file_has "$T/spine.log" "webid=https://id.lightlifeurbangardens.com/kade/profile/card#me"
  file_has "$T/spine.log" "jti=jti-kade-0001"
  file_lacks "$T/spine.log" "eyJhbGciOiJFUzI1NiJ9"
}

# ---- #4215 — the posture changed: a login failure must not cost Jeff the role ----
#
# Jeff, 2026-09-19: "chorus-awake must be a non issue day to day — highly
# reliable and resilient." The four tests that used to live here asserted the
# opposite contract — every login failure refuses — and that contract is what
# left Kade unreachable for an hour on 2026-09-18. They are rewritten, not
# deleted: the same inputs, the opposite expectation, except the one that still
# refuses.
#
# `nostart` and `lacks` are simple commands. `! grep` and `[[ ]]` mid-block both
# pass on bash 3.2 regardless of what they assert.
nostart() { test ! -f "$T/tmux.log"; }
started() { grep -q "respawn-pane -k -t chorus-kade" "$T/tmux.log"; }
lacks()   { test -z "$(grep -F -- "$2" "$1" 2>/dev/null || true)"; }
lacks_out() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }

# #4933 — Jeff 2026-11-08 17:24, asked "if a role's sign-in fails, does it still
# start?": "no". #4215's "starts anyway" is superseded: the role's wrapper runs
# `establish --run`, which fails with its reason, and the runtime never starts
# (establish_wrapper_4933.rs proves the wrapper half). A turn's establish (no
# --run) still degrades loudly and retries on the next turn.
no_run() { test "$status" -ne 0 && lacks_out "kade-run-1a2b"; }

@test "#4215→#4933 no token → the role does NOT start, and says why" {
  touch "$T/token-fail"
  est kade
  run "$SCRIPT" establish kade --run 1a2b
  out_has "no credential for 'kade'"
  no_run
}

@test "#4215 no token on a turn: degraded and loud, retried next turn" {
  touch "$T/token-fail"
  est kade
  run "$SCRIPT" establish kade
  test "$status" -eq 0
  out_has "pending"
  out_has "no credential for 'kade'"
  out_has "next turn retries"
}

@test "#4215 a degraded login is on the spine as session.login.degraded" {
  touch "$T/token-fail"
  est kade
  run "$SCRIPT" establish kade
  grep -q "^session.login.degraded kade " "$T/spine.log"
}

@test "#4215→#4933 the security API refusing the row stops the start, and says so" {
  echo 403 > "$T/curl.status"
  est kade
  run "$SCRIPT" establish kade --run 1a2b
  out_has "403"
  no_run
  # #4215 — found in the live pair: this line used to say "recorded yes" two
  # lines under the error. A start line that contradicts the error above it is
  # worse than no line.
  test -z "$(printf '%s' "$output" | grep -F "recorded yes" || true)"
  test -z "$(printf '%s' "$output" | grep -F " logged in " || true)"
  grep -q "^session.login.degraded kade " "$T/spine.log"
}

@test "#4215→#4933 an expired token stops the start: a stale credential, not someone else's" {
  one_token kade $(( $(date +%s) - 5 ))
  est kade
  run "$SCRIPT" establish kade --run 1a2b
  out_has "expired"
  lacks_out "wrong principal"
  no_run
}

@test "#4215 a 409 on a row this principal owns is a login, not a failure — the role starts" {
  echo 409 > "$T/curl.status.identity_sessions"   # the session row is the one that collides
  printf '{"name":"kade-0001-x","ownedBy":"principal-kade","sessionState":"open"}' > "$T/existing.json"
  est kade
  # #4933 (MOVE): "starts" is the wrapper's establish succeeding with a run to start on
  run "$SCRIPT" establish kade --run 1a2b
  [ "$status" -eq 0 ]
  out_has "already open and owned by principal-kade"
  out_has "reusing it"
  out_has "kade-run-1a2b"
}

@test "#4215 a 409 on ANOTHER principal's row still refuses, and says whose" {
  echo 409 > "$T/curl.status"
  printf '{"name":"kade-0001-x","ownedBy":"principal-silas","sessionState":"open"}' > "$T/existing.json"
  est kade
  run "$SCRIPT" establish kade --run 1a2b
  test "$status" -eq 1
  out_has "REFUSED"
  out_has "is NOT yours"
  out_has "principal-silas"
  no_run
  test -z "$(grep -F 'identity/sessionruns' "$T/curl.log" | grep -E -- '-X POST' || true)"
}

@test "#4215 THE ONE REFUSAL — another role's WebID is refused and nothing is started" {
  one_token silas
  est kade
  : > "$T/curl.log"
  run "$SCRIPT" establish kade --run 1a2b
  test "$status" -eq 1
  out_has "REFUSED"
  out_has "wrong principal"
  out_has "silas"
  out_has "file this work as theirs"
  no_run
  test -z "$(grep -E -- '-X POST' "$T/curl.log" || true)"
}

@test "#4933 one posture: the retired AWAKE_REFUSE_ON_LOGIN_FAILURE switch changes nothing" {
  # Jeff 2026-11-08 17:24 ("no"): a failed sign-in does not start the role. The
  # old toggle back to another posture is removed; setting it must not matter.
  touch "$T/token-fail"
  est kade
  run "$SCRIPT" establish kade --run 1a2b
  plain_status=$status; plain_out=$output
  rm -f "$T/identity/kade/login.json"
  run env AWAKE_REFUSE_ON_LOGIN_FAILURE=1 "$SCRIPT" establish kade --run 1a2b
  test "$plain_status" -ne 0
  test "$status" -eq "$plain_status"
  test "$output" = "$plain_out"
  lacks_out "pre-#4215 posture"
}

@test "idempotent: a role that is awake AND talking is not logged in again (nothing sent)" {
  # #4215 — a registry entry alone is no longer "awake": the entry outlives the
  # conversation, which is how a mute Kade read as present. The role has to have
  # SPOKEN, so this test writes a turn onto the spine. The mute case is the next
  # test.
  # #4295 — and its login is on file for THIS pid; without that it is logged in
  # (4295-role-login.bats holds that case).
  reg 56344 %0
  printf '{"state":"recorded","session":"kade-x-1","pid":56344}' > "$T/identity/kade/login.json"
  printf '{"role":"kade","event":"reply.published","timestamp":"%s"}\n' "$(date '+%Y-%m-%dT%H:%M:%S')" > "$T/spine-read.log"
  run env CHORUS_LOG_FILE="$T/spine-read.log" "$SCRIPT" kade
  test "$status" -eq 0
  test ! -f "$T/token.log"
  # #4368: the one call is the Principal row read; nothing is written
  grep -q -- '/v1/identity/principals/kade' "$T/curl.log"
  test -z "$(grep -E -- '-X (POST|PUT)' "$T/curl.log" 2>/dev/null || true)"
}

@test "#4215 a registered but MUTE session is replaced, not blessed" {
  reg 56345 %0
  printf '{"role":"kade","event":"reply.published","timestamp":"2026-01-01T00:00:00"}\n' > "$T/spine-read.log"
  run env CHORUS_LOG_FILE="$T/spine-read.log" "$SCRIPT" kade
  grep -q "stop 56345" "$T/claude.log"
  test ! -f "$T/sessions/kade-56345.json"
}

@test "#4215 NEGATIVE PROOF — without the liveness check the same mute session is blessed" {
  reg 56346 %0
  printf '{"role":"kade","event":"reply.published","timestamp":"2026-01-01T00:00:00"}\n' > "$T/spine-read.log"
  run env CHORUS_LOG_FILE="$T/spine-read.log" AWAKE_LIVENESS=0 "$SCRIPT" kade
  test "$status" -eq 0
  test -f "$T/sessions/kade-56346.json"
  test -z "$(grep -F "stop 56346" "$T/claude.log" 2>/dev/null || true)"
}
