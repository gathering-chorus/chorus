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
for a in "\$@"; do case "\$a" in @*) cp "\${a#@}" "$T/curl.body" ;; esac; done
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
  unset TMUX
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

@test "NEGATIVE PROOF — no token → REFUSED naming 'no session', and NOTHING is started" {
  touch "$T/token-fail"
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  out_has "REFUSED"
  out_has "no session for kade"
  out_has "no credential for 'kade'"
  [ ! -f "$T/tmux.log" ]
  [ ! -f "$T/curl.log" ]
}

@test "NEGATIVE PROOF — a token for ANOTHER role's WebID is refused as 'wrong principal'; nothing started" {
  mk_token silas
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  out_has "REFUSED"
  out_has "wrong principal"
  out_has "silas"
  [ ! -f "$T/tmux.log" ]
}

@test "NEGATIVE PROOF — an expired token is not a login" {
  mk_token kade $(( $(date +%s) - 5 ))
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  out_has "REFUSED"
  out_has "expired"
  [ ! -f "$T/tmux.log" ]
}

@test "NEGATIVE PROOF — the security API refuses the Session row → not logged in, nothing started" {
  echo 403 > "$T/curl.status"
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  out_has "REFUSED"
  out_has "login not recorded"
  out_has "403"
  [ ! -f "$T/tmux.log" ]
  [ ! -f "$T/spine.log" ]
}

@test "idempotent: an already-awake role is not logged in again (nothing sent)" {
  reg 56344 %0
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  [ ! -f "$T/token.log" ]
  [ ! -f "$T/curl.log" ]
}
