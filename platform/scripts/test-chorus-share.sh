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

# #3770 — the guard authenticates people, so the harness needs an identity world:
# a WebID allow-set, a private state file (cookie key), and OFFLINE mode so no
# live identity provider is contacted. Sessions are minted with the guard's OWN
# signing code via --sign-session, never a reimplementation of it.
PRINCIPALS="$TEST_ROOT/principals.txt"
printf 'https://example.test/alice#me\n' > "$PRINCIPALS"
STATE="$TEST_ROOT/oidc-state.json"
export SHARE_PRINCIPALS_FILE="$PRINCIPALS" SHARE_STATE_FILE="$STATE" SHARE_OIDC_OFFLINE=1

# guard: allow /about only
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" \
  SHARE_PORT="$G_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
G_PID=$!
for i in $(seq 1 20); do
  curl -s -o /dev/null "http://127.0.0.1:$G_PORT/" 2>/dev/null && break
  sleep 0.3
done

code() { curl -s -o /dev/null -w "%{http_code}" "$@"; }

# Sessions are minted by the guard's OWN signing code (--sign-session) so these
# tests exercise the real path. A second implementation of session signing would
# happily agree with the first while both were wrong.
mint() { python3 "$GUARD" --sign-session "$1"; }
SESS_ALICE="$(mint 'https://example.test/alice#me')"   # quoted: bare # would start a comment
SESS_STRANGER="$(mint 'https://example.test/mallory#me')"
as_alice()      { curl -s -H "Cookie: chorus_share_session=$SESS_ALICE" "$@"; }
code_alice()    { curl -s -o /dev/null -w "%{http_code}" -H "Cookie: chorus_share_session=$SESS_ALICE" "$@"; }
code_stranger() { curl -s -o /dev/null -w "%{http_code}" -H "Cookie: chorus_share_session=$SESS_STRANGER" "$@"; }

# auth
assert "no session -> redirected to sign-in (302)" test "$(code http://127.0.0.1:$G_PORT/about/x.html)" = "302"
assert "NEGATIVE PROOF: tampered session cookie is not a session" test "$(code -H "Cookie: chorus_share_session=not.a.real.session" http://127.0.0.1:$G_PORT/about/x.html)" = "302"
assert "authorized session + allowed path -> 200" test "$(code_alice http://127.0.0.1:$G_PORT/about/x.html)" = "200"
BODY=$(as_alice "http://127.0.0.1:$G_PORT/about/x.html")
assert "body proxied from upstream" test "$BODY" = "public page"

# allowlist
assert "non-allowlisted path -> 404 (never proxied)" test "$(code_alice http://127.0.0.1:$G_PORT/secret/y.html)" = "404"
assert "root -> 404 when only /about is shared" test "$(code_alice http://127.0.0.1:$G_PORT/)" = "404"
assert "prefix trickery /aboutX -> 404" test "$(code_alice http://127.0.0.1:$G_PORT/aboutX)" = "404"

# read-only: every write verb refused at the guard
for verb in POST PUT DELETE PATCH; do
  assert "$verb -> 405 (write refused at guard)" \
    test "$(code_alice -X "$verb" http://127.0.0.1:$G_PORT/about/x.html)" = "405"
done

# HEAD allowed
assert "HEAD allowed on shared path" test "$(code_alice -I http://127.0.0.1:$G_PORT/about/x.html)" = "200"

# encoding: a browser-style Accept-Encoding request must get READABLE bytes
# (the guard strips Accept-Encoding upstream — the mojibake bug, live 2026-07-14)
BODY_GZ=$(as_alice --compressed -H "Accept-Encoding: gzip, br" "http://127.0.0.1:$G_PORT/about/x.html")
assert "browser-style gzip request gets readable body" test "$BODY_GZ" = "public page"

# guard refuses to start unauthenticated
SHARE_AUTH="tester:pw123" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: basic auth RETIRED — setting SHARE_AUTH refuses to start (exit 2)" test "$?" -eq 2

# guard refuses a non-loopback bind (Silas #3644: fail-closed on misconfiguration)
SHARE_BIND="0.0.0.0" SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "guard refuses non-loopback bind (exit 2)" test "$?" -eq 2

# --- #3744: the allowlist is a GOVERNED FILE, and an absent policy fails closed ---

# The 2026-07-22 shape: policy only in a process env, nothing on disk. Now the
# file is the source of truth and the guard reads it.
ALLOWFILE="$TEST_ROOT/allow.txt"
printf '# comment ignored\n/about\n\n/extra   # trailing comment\n' > "$ALLOWFILE"
G2_PORT=$((G_PORT + 1))
mkdir -p "$TEST_ROOT/www/extra"; echo "extra page" > "$TEST_ROOT/www/extra/e.html"
env -u SHARE_ALLOW SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW_FILE="$ALLOWFILE" \
  SHARE_PORT="$G2_PORT" python3 "$GUARD" >/dev/null 2>&1 &
G2_PID=$!
for i in $(seq 1 20); do curl -s -o /dev/null "http://127.0.0.1:$G2_PORT/" 2>/dev/null && break; sleep 0.3; done
assert "file allowlist: /about served" test "$(code_alice http://127.0.0.1:$G2_PORT/about/x.html)" = "200"
assert "file allowlist: second entry served" test "$(code_alice http://127.0.0.1:$G2_PORT/extra/e.html)" = "200"
assert "file allowlist: comments/blank lines ignored, not treated as paths" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/secret/y.html)" = "404"
kill "$G2_PID" 2>/dev/null

# NEGATIVE PROOF: no file and no env → REFUSE TO START. Never default to "/",
# which would silently expose the whole upstream through a public tunnel.
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/does-not-exist.txt" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: missing allowlist file -> refuse to start (exit 2), never allow-all" test "$?" -eq 2

# NEGATIVE PROOF: a file that exists but declares nothing is a misconfiguration,
# not an empty policy that quietly permits everything.
: > "$TEST_ROOT/empty.txt"
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/empty.txt" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: empty allowlist file -> refuse to start (exit 2)" test "$?" -eq 2

# The committed allowlist must actually carry the Athena entries Jeff asked for
# (#3744) — otherwise the card's own deliverable can regress unnoticed.
REPO_ALLOW="$SCRIPT_DIR/../../config/share-allowlist.txt"
assert "committed allowlist exists" test -f "$REPO_ALLOW"
# First field only — #3767 lets a line carry an upstream after the prefix.
prefixes() { sed 's/#.*//' "$REPO_ALLOW" | awk 'NF {print $1}'; }
for want in /about /athena /domains /owl; do
  assert "committed allowlist carries $want" sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; prefixes | grep -qx -- '$want'"
done

# --- #3767: per-prefix upstream, so /about (:3000) and Athena (:3340) coexist ---

# Two DISTINCT upstreams serving distinguishable content. This is the whole point
# of the card: one global upstream could only ever serve one of the two shares,
# and on 2026-08-06 pointing it at :3340 published Athena and 404'd /about.
UP2_PORT=$((G_PORT + 10))
mkdir -p "$TEST_ROOT/www2/about" "$TEST_ROOT/www2/athena"
echo "FROM-UPSTREAM-TWO" > "$TEST_ROOT/www2/about/x.html"
echo "athena page"       > "$TEST_ROOT/www2/athena/model.html"
(cd "$TEST_ROOT/www2" && python3 -m http.server "$UP2_PORT" >/dev/null 2>&1) &
UP2_PID=$!
for i in $(seq 1 20); do curl -s -o /dev/null "http://127.0.0.1:$UP2_PORT/" 2>/dev/null && break; sleep 0.3; done

# /about is pinned to upstream TWO; /athena has no route so it falls to the
# default (upstream ONE, which has no /athena and therefore 404s). The asymmetry
# is deliberate: it proves the pin is doing the routing, not coincidence.
ROUTEFILE="$TEST_ROOT/routes.txt"
printf '/about http://127.0.0.1:%s\n/athena\n' "$UP2_PORT" > "$ROUTEFILE"
G3_PORT=$((G_PORT + 11))
env -u SHARE_ALLOW SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW_FILE="$ROUTEFILE" \
  SHARE_PORT="$G3_PORT" python3 "$GUARD" >/dev/null 2>&1 &
G3_PID=$!
for i in $(seq 1 20); do curl -s -o /dev/null "http://127.0.0.1:$G3_PORT/" 2>/dev/null && break; sleep 0.3; done

ROUTED=$(as_alice "http://127.0.0.1:$G3_PORT/about/x.html")
assert "routed prefix is served from ITS upstream, not the default" \
  test "$ROUTED" = "FROM-UPSTREAM-TWO"
# NEGATIVE PROOF for the routing itself: if the pin were ignored and everything
# went to the default, /about/x.html would return upstream ONE's "public page".
# That string must NOT appear.
assert "NEGATIVE PROOF: routed prefix does NOT fall through to the default upstream" \
  test "$ROUTED" != "public page"
# And the converse: an unrouted prefix DOES use the default, which here lacks the
# file — so a 404 proves it went to ONE and not to TWO (where the file exists).
assert "NEGATIVE PROOF: unrouted prefix uses the default upstream, not a routed one" \
  test "$(code_alice http://127.0.0.1:$G3_PORT/athena/model.html)" = "404"
assert "routing does not weaken the allowlist: unlisted path still 404s" \
  test "$(code_alice http://127.0.0.1:$G3_PORT/secret/y.html)" = "404"
assert "routing does not weaken auth: unauthenticated still refused (sent to sign-in)" \
  test "$(code http://127.0.0.1:$G3_PORT/about/x.html)" = "302"
assert "routing does not weaken read-only: POST still 405s" \
  test "$(code_alice -X POST http://127.0.0.1:$G3_PORT/about/x.html)" = "405"
kill "$G3_PID" 2>/dev/null

# NEGATIVE PROOF: an off-box upstream must STOP the guard. A typo here turns the
# public tunnel into a proxy for another host — a reachability change no one
# reviewing an allowlist expects to be making.
printf '/about http://192.168.86.242:3000\n' > "$TEST_ROOT/offbox.txt"
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/offbox.txt" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: non-loopback upstream in allowlist -> refuse to start (exit 2)" test "$?" -eq 2

# Same rule applies to the DEFAULT upstream, not just per-prefix routes.
printf '/about\n' > "$TEST_ROOT/okprefix.txt"
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/okprefix.txt" SHARE_UPSTREAM="http://example.com" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: non-loopback SHARE_UPSTREAM -> refuse to start (exit 2)" test "$?" -eq 2

# A non-http scheme is refused too (file:// would read local disk).
printf '/about file:///etc\n' > "$TEST_ROOT/scheme.txt"
env -u SHARE_ALLOW SHARE_ALLOW_FILE="$TEST_ROOT/scheme.txt" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: non-http upstream scheme -> refuse to start (exit 2)" test "$?" -eq 2

kill "$UP2_PID" 2>/dev/null

# The committed policy must actually pin /about to the app (:3000) — otherwise
# the fix regresses the moment someone tidies the file.
assert "committed allowlist routes /about to :3000" \
  grep -qE '^/about[[:space:]]+http://localhost:3000[[:space:]]*$' "$REPO_ALLOW"
# ...and the committed plist must default to :3340, or Athena goes dark again.
REPO_PLIST="$SCRIPT_DIR/../../config/launchagents/com.chorus.share-guard.plist"
assert "committed plist defaults upstream to :3340" \
  grep -q '<string>http://localhost:3340</string>' "$REPO_PLIST"

# #3744's finding, generalized: config that only exists on the live box cannot be
# reviewed. If the agent is installed, it must MATCH the committed one.
INSTALLED_PLIST="$HOME/Library/LaunchAgents/com.chorus.share-guard.plist"
if [ -f "$INSTALLED_PLIST" ]; then
  assert "installed plist matches the committed one (no live-only config)" \
    diff -q "$REPO_PLIST" "$INSTALLED_PLIST"
else
  echo "SKIP: share-guard agent not installed on this box — cannot compare live config"
fi

# --- #3770: authentication is not authorization, and revocation is immediate ---

# NEGATIVE PROOF: a perfectly valid session for someone NOT in the allow-set is
# refused. Signing in proves who you are; it grants nothing.
assert "NEGATIVE PROOF: authenticated but not in the allow-set -> 403" \
  test "$(code_stranger http://127.0.0.1:$G_PORT/about/x.html)" = "403"

# NEGATIVE PROOF: removing a line revokes on the NEXT request — not at session
# expiry, and without disturbing anyone else. Alice's own session is untouched
# throughout; only the file changes.
printf 'https://example.test/alice#me\nhttps://example.test/mallory#me\n' > "$PRINCIPALS"
G4_PORT=$((G_PORT + 20))
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$G4_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
G4_PID=$!
for i in $(seq 1 20); do curl -s -o /dev/null "http://127.0.0.1:$G4_PORT/" 2>/dev/null && break; sleep 0.3; done
assert "granted: a WebID added to the allow-set is served" \
  test "$(code_stranger http://127.0.0.1:$G4_PORT/about/x.html)" = "200"
printf 'https://example.test/alice#me\n' > "$PRINCIPALS"
assert "NEGATIVE PROOF: revoked WebID is refused on the very next request" \
  test "$(code_stranger http://127.0.0.1:$G4_PORT/about/x.html)" = "403"
assert "revocation is targeted: the other person is unaffected" \
  test "$(code_alice http://127.0.0.1:$G4_PORT/about/x.html)" = "200"
kill "$G4_PID" 2>/dev/null

# NEGATIVE PROOF: an empty allow-set is a misconfiguration, never "anyone who can
# sign in" — the #3744 fail-closed shape applied to WHO instead of WHAT.
: > "$TEST_ROOT/empty-principals.txt"
env SHARE_PRINCIPALS_FILE="$TEST_ROOT/empty-principals.txt" SHARE_ALLOW="/about" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: empty WebID allow-set -> refuse to start (exit 2)" test "$?" -eq 2
env SHARE_PRINCIPALS_FILE="$TEST_ROOT/no-such-principals.txt" SHARE_ALLOW="/about" \
  SHARE_PORT=1 python3 "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: missing WebID allow-set -> refuse to start (exit 2)" test "$?" -eq 2

# A WebID ends in a fragment. This is the bug the suite caught during #3770: the
# '#' comment rule truncated every entry, so the allow-set matched nobody while
# reading correctly. Pin it.
printf '# a comment line\nhttps://example.test/alice#me   # trailing note\n' > "$TEST_ROOT/frag.txt"
env SHARE_PRINCIPALS_FILE="$TEST_ROOT/frag.txt" python3 - <<'"'"'PYEOF'"'"'
import os, sys
sys.argv = ["guard"]
src = open(os.environ.get("GUARD_PATH")).read().split("ALLOW, ALLOW_SOURCE = load_allow()")[0]
ns = {"__name__": "notmain"}
exec(compile(src, "guard", "exec"), ns)
got, _ = ns["load_principals"]()
sys.exit(0 if got == ["https://example.test/alice#me"] else 1)
PYEOF
assert "WebID fragments survive comment-stripping (#me is not a comment)" test "$?" -eq 0

# The committed allow-set must actually name Jeff, or the surface is unreachable
# by the person it exists for.
REPO_PRINCIPALS="$SCRIPT_DIR/../../config/share-principals.txt"
assert "committed allow-set exists" test -f "$REPO_PRINCIPALS"
assert "committed allow-set names Jeff" grep -q 'jeff/profile/card#me' "$REPO_PRINCIPALS"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
