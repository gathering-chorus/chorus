#!/usr/bin/env bash
# test-chorus-share.sh — hermetic tests for the share guard (#3644).
#
# Brings its own world: a stub upstream on a random port, the guard on another,
# no Caddy, no tunnel, no live services. The tunnel itself (cloudflared) is a
# named boundary — its proof is the live demo; everything the GUARD guarantees
# (auth, read-only, allowlist — the actual security posture) is pinned here.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUARD="$SCRIPT_DIR/chorus-share-guard.py"

PASS=0; FAIL=0
assert() {
  local label="$1"; shift
  if "$@"; then PASS=$((PASS+1)); echo "PASS: $label"
  else FAIL=$((FAIL+1)); echo "FAIL: $label"; fi
}

TEST_ROOT=$(mktemp -d)
UP_PORT=$(( (RANDOM % 2000) + 42000 ))
G_PORT=$(( UP_PORT + 1 ))
UP_PID=""; G_PID=""
cleanup() {
  [ -n "$UP_PID" ] && kill "$UP_PID" 2>/dev/null
  [ -n "$G_PID" ] && kill "$G_PID" 2>/dev/null
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

# stub upstream: serves /about/x.html and /secret/y.html
mkdir -p "$TEST_ROOT/www/about" "$TEST_ROOT/www/secret"
echo "public page" > "$TEST_ROOT/www/about/x.html"
echo "not shared" > "$TEST_ROOT/www/secret/y.html"
(cd "$TEST_ROOT/www" && python3 -m http.server "$UP_PORT" >/dev/null 2>&1) &
UP_PID=$!

# guard: allow /about only, auth tester:pw123
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" \
  SHARE_AUTH="tester:pw123" SHARE_PORT="$G_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
G_PID=$!
for i in $(seq 1 20); do
  curl -s -o /dev/null "http://127.0.0.1:$G_PORT/" 2>/dev/null && break
  sleep 0.3
done

code() { curl -s -o /dev/null -w "%{http_code}" "$@"; }

# auth
assert "no auth -> 401" test "$(code http://127.0.0.1:$G_PORT/about/x.html)" = "401"
assert "wrong auth -> 401" test "$(code -u tester:WRONG http://127.0.0.1:$G_PORT/about/x.html)" = "401" # gitleaks:allow — fixture cred, hermetic stub
assert "good auth + allowed path -> 200" test "$(code -u tester:pw123 http://127.0.0.1:$G_PORT/about/x.html)" = "200" # gitleaks:allow — fixture cred, hermetic stub
BODY=$(curl -s -u tester:pw123 "http://127.0.0.1:$G_PORT/about/x.html") # gitleaks:allow — fixture cred
assert "body proxied from upstream" test "$BODY" = "public page"

# allowlist
assert "non-allowlisted path -> 404 (never proxied)" test "$(code -u tester:pw123 http://127.0.0.1:$G_PORT/secret/y.html)" = "404"
assert "root -> 404 when only /about is shared" test "$(code -u tester:pw123 http://127.0.0.1:$G_PORT/)" = "404"
assert "prefix trickery /aboutX -> 404" test "$(code -u tester:pw123 http://127.0.0.1:$G_PORT/aboutX)" = "404"

# read-only: every write verb refused at the guard
for verb in POST PUT DELETE PATCH; do
  assert "$verb -> 405 (write refused at guard)" \
    test "$(code -u tester:pw123 -X "$verb" http://127.0.0.1:$G_PORT/about/x.html)" = "405"
done

# HEAD allowed
assert "HEAD allowed on shared path" test "$(code -u tester:pw123 -I http://127.0.0.1:$G_PORT/about/x.html)" = "200"

# encoding: a browser-style Accept-Encoding request must get READABLE bytes
# (the guard strips Accept-Encoding upstream — the mojibake bug, live 2026-07-14)
BODY_GZ=$(curl -s --compressed -H "Accept-Encoding: gzip, br" -u tester:pw123 "http://127.0.0.1:$G_PORT/about/x.html") # gitleaks:allow — fixture cred
assert "browser-style gzip request gets readable body" test "$BODY_GZ" = "public page"

# guard refuses to start unauthenticated
SHARE_AUTH="" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "guard refuses to start without SHARE_AUTH (exit 2)" test "$?" -eq 2

# guard refuses a non-loopback bind (Silas #3644: fail-closed on misconfiguration)
SHARE_AUTH="t:p" SHARE_BIND="0.0.0.0" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "guard refuses non-loopback bind (exit 2)" test "$?" -eq 2

# --- #3744: the allowlist is a GOVERNED FILE, and an absent policy fails closed ---

# The 2026-07-22 shape: policy only in a process env, nothing on disk. Now the
# file is the source of truth and the guard reads it.
ALLOWFILE="$TEST_ROOT/allow.txt"
printf '# comment ignored\n/about\n\n/extra   # trailing comment\n' > "$ALLOWFILE"
G2_PORT=$((G_PORT + 1))
mkdir -p "$TEST_ROOT/www/extra"; echo "extra page" > "$TEST_ROOT/www/extra/e.html"
env -u SHARE_ALLOW SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW_FILE="$ALLOWFILE" \
  SHARE_AUTH="tester:pw123" SHARE_PORT="$G2_PORT" python3 "$GUARD" >/dev/null 2>&1 &
G2_PID=$!
for i in $(seq 1 20); do curl -s -o /dev/null "http://127.0.0.1:$G2_PORT/" 2>/dev/null && break; sleep 0.3; done
assert "file allowlist: /about served" test "$(code -u tester:pw123 http://127.0.0.1:$G2_PORT/about/x.html)" = "200" # gitleaks:allow — fixture cred
assert "file allowlist: second entry served" test "$(code -u tester:pw123 http://127.0.0.1:$G2_PORT/extra/e.html)" = "200" # gitleaks:allow — fixture cred
assert "file allowlist: comments/blank lines ignored, not treated as paths" \
  test "$(code -u tester:pw123 http://127.0.0.1:$G2_PORT/secret/y.html)" = "404"
kill "$G2_PID" 2>/dev/null

# NEGATIVE PROOF: no file and no env → REFUSE TO START. Never default to "/",
# which would silently expose the whole upstream through a public tunnel.
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/does-not-exist.txt" \
  SHARE_AUTH="t:p" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: missing allowlist file -> refuse to start (exit 2), never allow-all" test "$?" -eq 2

# NEGATIVE PROOF: a file that exists but declares nothing is a misconfiguration,
# not an empty policy that quietly permits everything.
: > "$TEST_ROOT/empty.txt"
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/empty.txt" \
  SHARE_AUTH="t:p" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: empty allowlist file -> refuse to start (exit 2)" test "$?" -eq 2

# The committed allowlist must actually carry the Athena entries Jeff asked for
# (#3744) — otherwise the card's own deliverable can regress unnoticed.
REPO_ALLOW="$SCRIPT_DIR/../../config/share-allowlist.txt"
assert "committed allowlist exists" test -f "$REPO_ALLOW"
for want in /about /athena /domains /owl; do
  assert "committed allowlist carries $want" grep -qx -- "$want" "$REPO_ALLOW"
done

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
