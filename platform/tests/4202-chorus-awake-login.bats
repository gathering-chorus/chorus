#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake binary with stub claude, tmux, ps, token-minter, curl and chorus-log; no live services.
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
  SCRIPT="${CHORUS_AWAKE_BIN:-$ROOT/platform/services/chorus-awake/target/release/chorus-awake}"
  [ -x "$SCRIPT" ] || skip "chorus-awake not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/sessions" "$T/bin" "$T/roles/kade" "$T/projects" "$T/identity/kade"
  touch "$T/alive-pids"
  cat > "$T/bin/ps" <<EOS
#!/bin/bash
grep -qx "\$2" "$T/alive-pids"
EOS
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
case "\$1" in has-session) [ -f "$T/tmux-session-exists" ]; exit \$? ;; new-session) touch "$T/tmux-session-exists" ;; esac
exit 0
EOS
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
if [ "\$1" = "agents" ]; then echo '[]'; fi
exit 0
EOS
  # stub token minter: prints the fixture token for the role asked, or refuses
  cat > "$T/bin/token" <<EOS
#!/bin/bash
echo "token \$*" >> "$T/token.log"
[ -f "$T/token-fail" ] && { echo "chorus-identity-token: no credential for '\$1'" >&2; exit 3; }
cat "$T/token.fixture"
EOS
  # stub curl: records the request (never the token), answers the fixture status
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
echo "curl \$*" >> "$T/curl.log"
# the 409 branch re-reads the row with a plain GET (no -X POST): answer it with
# the fixture body so the ownership decision can be exercised. #4215.
case "\$*" in *"-X POST"*) ;; *) cat "$T/existing.json" 2>/dev/null; exit 0 ;; esac
# #4328 — a login now also POSTs its run, presence and context; keep the SESSION body

for a in "\$@"; do case "\$a" in @*session.body) cp "\${a#@}" "$T/curl.body" ;; esac; done
cat "$T/curl.status" 2>/dev/null || echo 201
EOS
  cat > "$T/bin/chorus-log" <<EOS
#!/bin/bash
echo "\$*" >> "$T/spine.log"
EOS
  chmod +x "$T/bin/"*
  export CLAUDE_BIN="$T/bin/claude" TMUX_BIN="$T/bin/tmux" AWAKE_PS="$T/bin/ps"
  export CHORUS_TOKEN_BIN="$T/bin/token" AWAKE_CURL="$T/bin/curl" CHORUS_LOG_BIN="$T/bin/chorus-log"
  export CHORUS_IDENTITY_DIR="$T/identity" CHORUS_API_URL="http://stub:1"
  export CHORUS_SESSIONS_DIR="$T/sessions" AWAKE_ROLE_DIR="$T/roles/kade" CHORUS_ROOT="$ROOT"
  export AWAKE_PROJECTS_DIR="$T/projects" AWAKE_NO_ATTACH=1 AWAKE_WAIT=1 USER=unit-account
  unset TMUX CLAUDECODE
  # #4295: no live service probes from a unit world; no background retry left running
  export AWAKE_SERVICES=none AWAKE_NO_RETRY=1
  mk_token kade
}

# a JWT-shaped token whose payload names <role>'s WebID; the signature is not checked here (CSS signs, the API verifies)
mk_token() {
  local role="$1" exp="${2:-$(( $(date +%s) + 600 ))}"
  local payload
  payload=$(printf '{"webid":"https://id.lightlifeurbangardens.com/%s/profile/card#me","jti":"jti-%s-0001","iat":%s,"exp":%s}' "$role" "$role" "$(date +%s)" "$exp" | base64 | tr '+/' '-_' | tr -d '=\n')
  printf 'eyJhbGciOiJFUzI1NiJ9.%s.sig' "$payload" > "$T/token.fixture"
}

reg() { printf '{"role":"kade","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%s"}' "$1" "${1: -1}" "${2:-%0}" > "$T/sessions/kade-$1.json"; echo "$1" >> "$T/alive-pids"; }
out_has() { printf '%s' "$output" | grep -q -- "$1"; }
file_has() { grep -q -- "$2" "$1"; }
file_lacks() { ! grep -q -- "$2" "$1"; }

@test "login happens BEFORE the pane is started: token asked for the role, then claude launched" {
  ( sleep 0.3; reg 777 %5 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  file_has "$T/token.log" "token kade"
  file_has "$T/tmux.log" "send-keys -t chorus-kade"
  # the pane gets the token FILE, never the token on the command line
  file_has "$T/tmux.log" "CHORUS_SESSION_TOKEN_FILE='$T/identity/kade/token.cache'"
  file_lacks "$T/tmux.log" "eyJhbGciOiJFUzI1NiJ9"
}

@test "the login is recorded as a Session row through the security API, owned by the principal" {
  ( sleep 0.3; reg 778 %5 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  file_has "$T/curl.log" "http://stub:1/v1/identity/sessions"
  file_has "$T/curl.body" '"ownedBy":"principal-kade"'
  file_has "$T/curl.body" '"tokenId":"jti-kade-0001"'
  file_has "$T/curl.body" '"sessionState":"open"'
  file_has "$T/curl.body" '"hostAccount":"unit-account"'
  # the token travels as a header FILE, not as a curl argument
  file_lacks "$T/curl.log" "eyJhbGciOiJFUzI1NiJ9"
  out_has "login: kade"
  out_has "recorded yes"
}

@test "session.login is on the spine with the WebID and the token id, never the token" {
  ( sleep 0.3; reg 779 %5 ) &
  run "$SCRIPT" kade
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
started() { grep -q "send-keys -t chorus-kade" "$T/tmux.log"; }
lacks()   { test -z "$(grep -F -- "$2" "$1" 2>/dev/null || true)"; }

@test "#4215 no token → the role STARTS anyway, degraded and loud" {
  touch "$T/token-fail"
  ( sleep 0.3; reg 781 %5 ) &
  run "$SCRIPT" kade
  # #4295 — the words changed: a failed sign-in says it is PENDING and handles
  # itself, not "UNAUTHENTICATED ... any write will be refused" at Jeff
  out_has "login: kade  pending"
  out_has "no credential for 'kade'"
  out_has "logs itself in"
  started
}

@test "#4215 a degraded login is on the spine as session.login.degraded" {
  touch "$T/token-fail"
  ( sleep 0.3; reg 782 %5 ) &
  run "$SCRIPT" kade
  grep -q "^session.login.degraded kade " "$T/spine.log"
}

@test "#4215 the security API refusing the row does NOT stop the start" {
  echo 403 > "$T/curl.status"
  ( sleep 0.3; reg 783 %5 ) &
  run "$SCRIPT" kade
  out_has "login: kade  pending"
  out_has "403"
  started
  # #4215 — found in the live pair: this line used to say "recorded yes" two
  # lines under the error. A start line that contradicts the error above it is
  # worse than no line.
  test -z "$(printf '%s' "$output" | grep -F "recorded yes" || true)"
  test -z "$(printf '%s' "$output" | grep -F " logged in " || true)"
  grep -q "^session.login.degraded kade " "$T/spine.log"
}

@test "#4215 an expired token degrades: it is a stale credential, not someone else's" {
  mk_token kade $(( $(date +%s) - 5 ))
  ( sleep 0.3; reg 784 %5 ) &
  run "$SCRIPT" kade
  out_has "expired"
  out_has "login: kade  pending"
  started
}

@test "#4215 a 409 on a row this principal owns is a login, not a failure — the role starts" {
  echo 409 > "$T/curl.status"
  printf '{"name":"kade-0001-x","ownedBy":"principal-kade","sessionState":"open"}' > "$T/existing.json"
  ( sleep 0.3; reg 785 %5 ) &
  run "$SCRIPT" kade
  out_has "already open and owned by principal-kade"
  out_has "reusing it"
  started
}

@test "#4215 a 409 on ANOTHER principal's row still refuses, and says whose" {
  echo 409 > "$T/curl.status"
  printf '{"name":"kade-0001-x","ownedBy":"principal-silas","sessionState":"open"}' > "$T/existing.json"
  run "$SCRIPT" kade
  test "$status" -eq 1
  out_has "REFUSED"
  out_has "is NOT yours"
  out_has "principal-silas"
  nostart
}

@test "#4215 THE ONE REFUSAL — another role's WebID is refused and nothing is started" {
  mk_token silas
  run "$SCRIPT" kade
  test "$status" -eq 1
  out_has "REFUSED"
  out_has "wrong principal"
  out_has "silas"
  out_has "file this work as theirs"
  nostart
}

@test "#4215 NEGATIVE PROOF — under the OLD posture the SAME input starts nothing" {
  # AWAKE_REFUSE_ON_LOGIN_FAILURE=1 is the pre-#4215 behaviour, kept only so this
  # proof can show the two differ on the input that cost Jeff an hour of Kade.
  # If this test ever passes with a pane started, the degrade branch is dead code.
  touch "$T/token-fail"
  run env AWAKE_REFUSE_ON_LOGIN_FAILURE=1 "$SCRIPT" kade
  test "$status" -eq 1
  out_has "REFUSED"
  out_has "pre-#4215 posture"
  nostart
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
  test ! -f "$T/curl.log"
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
