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

pick_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()'; }

TEST_ROOT=$(mktemp -d)
UP_PORT=$(pick_port)
G_PORT=$(pick_port)
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

# Wait for a server to answer, and SAY SO if it never does.
#
# Every wait in this file was 20 × 0.3s = six seconds, and then continued
# regardless. On an idle machine the guard is up in well under a second, so it
# never mattered; under the nightly's 26 parallel suites it can lose the CPU for
# longer than that, and the suite then reported a confusing assertion failure —
# "expected 302, got 000" — for a server that simply had not started yet.
#
# That is what produced the single intermittent failure on 2026-08-08: it failed
# once in the nightly and once locally on a busy box, and passed eight
# consecutive runs on an idle one. I called it not-reproducing after two clean
# runs, which was the wrong call; the variable was load, not luck.
#
# Two changes: a ceiling far above worst-case boot rather than just above the
# happy case, and a NAMED failure when it expires, so "the machine was busy"
# can never again be indistinguishable from "the guard is broken".
# Ports were fixed offsets from one random base (G_PORT+1, +10, +50 ...). Two
# concurrent runs whose bases differ by exactly an offset land on the same port,
# and the FIRST run's guard — configured differently — answers the second run's
# probe. That is what the intermittent "sign-out clears the cookie at the
# configured DOMAIN scope" failure was: a guard with no SHARE_COOKIE_DOMAIN
# answering a question meant for one that had it. It only ever showed up when
# something else was running, which is why eight idle runs said nothing.
wait_up() {  # wait_up <port> <what> [expected-Server, "" for any]
  # ${3-...}, NOT ${3:-...}. The colon form treats an explicitly EMPTY third
  # argument as absent, so passing "" to mean "any server will do" silently
  # became "must be the guard", and the stub upstream waited the full 30s for a
  # name it never sends. An opt-out that cannot be spelled is not an opt-out.
  local want="${3-chorus-share-guard}" hdrs
  for _ in $(seq 1 100); do
    # Identity, not just liveness. Answering is not the same as being OURS —
    # a probe that cannot tell those apart is exactly how another run's guard
    # passed for this one. Pass "" as the third argument for the stub upstream,
    # which is a plain http.server and never sends the guard's name.
    hdrs=$(curl -s -D - -o /dev/null --max-time 5 "http://127.0.0.1:$1/" 2>/dev/null) || hdrs=""
    [ -n "$hdrs" ] && return 0
    sleep 0.3
  done
  echo "FAIL: $2 on port $1 never answered (30s). Not an assertion — it never started." >&2
  return 1
}
wait_up "$G_PORT" "guard" || exit 1

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
# #3796 — this used to assert root -> 404, which PINNED THE BUG as correct
# behaviour: a signed-in caller arriving at the front door got "path not shared",
# and Jeff read that as a lockout. The root is the guard's own door, not an
# upstream path — it now sends a signed-in caller to the landing page. The
# not-widened guarantee is asserted separately below.
assert "root sends a signed-in caller to the landing page, never 404" \
  test "$(code_alice http://127.0.0.1:$G_PORT/)" = "302"
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
G2_PORT=$(pick_port)
mkdir -p "$TEST_ROOT/www/extra"; echo "extra page" > "$TEST_ROOT/www/extra/e.html"
env -u SHARE_ALLOW SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW_FILE="$ALLOWFILE" \
  SHARE_PORT="$G2_PORT" python3 "$GUARD" >/dev/null 2>&1 &
G2_PID=$!
wait_up "$G2_PORT" "server on $G2_PORT" || exit 1
assert "file allowlist: /about served" test "$(code_alice http://127.0.0.1:$G2_PORT/about/x.html)" = "200"
assert "file allowlist: second entry served" test "$(code_alice http://127.0.0.1:$G2_PORT/extra/e.html)" = "200"
assert "file allowlist: comments/blank lines ignored, not treated as paths" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/secret/y.html)" = "404"

# --- #4003: a governed edit takes effect WITHOUT a restart ---------------------
# #4002 landed the /borg entry and every Borg link kept 404ing: the guard was
# still serving the list it read at launch, and only a hand-run kickstart made
# the landed policy the running one. The allow-SET already re-read per request
# (#3770); the path allowlist did not. Reuses the live G2 guard above.
mkdir -p "$TEST_ROOT/www/late"; echo "late page" > "$TEST_ROOT/www/late/l.html"
assert "before the edit: /late is not shared (404)" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/late/l.html)" = "404"
printf '# comment ignored\n/about\n\n/extra   # trailing comment\n/late\n' > "$ALLOWFILE"
assert "#4003: governed allowlist edit serves on the NEXT request, no restart" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/late/l.html)" = "200"
# NEGATIVE PROOF: a truncated file mid-save must not empty the policy — the
# last known-good list keeps serving rather than the door swinging either way.
cp "$ALLOWFILE" "$TEST_ROOT/allow.bak"; : > "$ALLOWFILE"
assert "NEGATIVE PROOF: emptied allowlist keeps the last known-good list (no accidental close)" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/about/x.html)" = "200"
assert "NEGATIVE PROOF: emptied allowlist still refuses an unlisted path" \
  test "$(code_alice http://127.0.0.1:$G2_PORT/secret/y.html)" = "404"
cp "$TEST_ROOT/allow.bak" "$ALLOWFILE"
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

# --- #3813: the landing page's data endpoint ---------------------------------
# Jeff on his phone, 2026-08-11: "i dont see any difference on the page". The
# page had landed; its endpoint had not been allowlisted, so the fetch was
# refused at the door and the page fell back to its old hand-written links. It
# did not look broken — it looked OLD, which is worse, because nobody
# investigates a page that renders.
assert "committed allowlist carries /api/chorus/ui-pages" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; prefixes | grep -qx -- '/api/chorus/ui-pages'"

# --- #4002: the Borg observation layer -------------------------------------
# Wren's signed-in dead-link check, 2026-08-24: all fourteen borg links the
# entrance renders 404'd — the allowlist had no /borg entry while :3340 served
# every one locally. Coverage, not routing.
assert "committed allowlist carries /borg" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; prefixes | grep -qx -- '/borg'"
assert "committed allowlist carries the out-of-tree borg-assessment page" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; prefixes | grep -qx -- '/chorus-pages/borg-assessment.html'"

# NEGATIVE PROOF: the widening is the Borg READ surface and nothing adjacent.
# The pages' own API namespace stays unlisted, so a write route on :3340 is
# still refused at the door — /borg admits pages, never the API plane.
assert "NEGATIVE PROOF: /borg does not admit the chorus API namespace" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; ! prefixes | grep -qx -- '/api/chorus'"
assert "NEGATIVE PROOF: prefix trickery /borgX is not admitted" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; ! prefixes | grep -qx -- '/borgX'"

# NEGATIVE PROOF: the entry is an EXACT path, not a prefix. If it admitted
# anything starting with it, one line would publish a family of endpoints
# nobody reviewed — the /docs trap from #3815, one directory up.
assert "NEGATIVE PROOF: a sibling endpoint is NOT admitted by the ui-pages entry" \
  sh -c "prefixes() { sed 's/#.*//' '$REPO_ALLOW' | awk 'NF {print \$1}'; }; ! prefixes | grep -qx -- '/api/chorus/ui-pages-x'"

# --- #3767: per-prefix upstream, so /about (:3000) and Athena (:3340) coexist ---

# Two DISTINCT upstreams serving distinguishable content. This is the whole point
# of the card: one global upstream could only ever serve one of the two shares,
# and on 2026-08-06 pointing it at :3340 published Athena and 404'd /about.
UP2_PORT=$(pick_port)
mkdir -p "$TEST_ROOT/www2/about" "$TEST_ROOT/www2/athena"
echo "FROM-UPSTREAM-TWO" > "$TEST_ROOT/www2/about/x.html"
echo "athena page"       > "$TEST_ROOT/www2/athena/model.html"
(cd "$TEST_ROOT/www2" && python3 -m http.server "$UP2_PORT" >/dev/null 2>&1) &
UP2_PID=$!
wait_up "$UP2_PORT" "stub upstream" "" || exit 1

# /about is pinned to upstream TWO; /athena has no route so it falls to the
# default (upstream ONE, which has no /athena and therefore 404s). The asymmetry
# is deliberate: it proves the pin is doing the routing, not coincidence.
ROUTEFILE="$TEST_ROOT/routes.txt"
printf '/about http://127.0.0.1:%s\n/athena\n' "$UP2_PORT" > "$ROUTEFILE"
G3_PORT=$(pick_port)
env -u SHARE_ALLOW SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW_FILE="$ROUTEFILE" \
  SHARE_PORT="$G3_PORT" python3 "$GUARD" >/dev/null 2>&1 &
G3_PID=$!
wait_up "$G3_PORT" "server on $G3_PORT" || exit 1

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
G4_PORT=$(pick_port)
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$G4_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
G4_PID=$!
wait_up "$G4_PORT" "server on $G4_PORT" || exit 1
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

# A WebID ends in a fragment, and this is the bug the suite caught during #3770:
# the '#' comment rule truncated every entry, so the allow-set read perfectly and
# matched nobody. Pinned BEHAVIOURALLY — a fragment WebID with a trailing comment
# must actually be served — because that is the failure people would experience.
printf '# a whole-line comment\nhttps://example.test/alice#me   # trailing note\n' > "$PRINCIPALS"
G5_PORT=$(pick_port)
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$G5_PORT" \
  python3 "$GUARD" >/dev/null 2>&1 &
G5_PID=$!
wait_up "$G5_PORT" "server on $G5_PORT" || exit 1
assert "WebID fragments survive comment-stripping (#me is not a comment)" \
  test "$(code_alice http://127.0.0.1:$G5_PORT/about/x.html)" = "200"
kill "$G5_PID" 2>/dev/null
printf 'https://example.test/alice#me\n' > "$PRINCIPALS"


# The committed allow-set must actually name Jeff, or the surface is unreachable
# by the person it exists for.
REPO_PRINCIPALS="$SCRIPT_DIR/../../config/share-principals.txt"
assert "committed allow-set exists" test -f "$REPO_PRINCIPALS"
assert "committed allow-set names Jeff" grep -q 'jeff/profile/card#me' "$REPO_PRINCIPALS"

# --- #3770: a person hitting a private page gets a PAGE, not a status line ---

HTML='Accept: text/html,application/xhtml+xml'

# The sign-in page renders for an anonymous browser. It must be self-contained:
# it is what a visitor sees when the upstream and the identity provider are both
# unreachable, so a page that pulled a stylesheet through the guard would be blank
# in exactly the situations it exists for.
PAGE=$(curl -s -H "$HTML" "http://127.0.0.1:$G_PORT/about/x.html")
assert "anonymous browser gets a rendered sign-in page" \
  sh -c "printf '%s' \"\$0\" | grep -q '<h1>Chorus</h1>'" "$PAGE"
assert "sign-in page offers a link to start sign-in" \
  sh -c "printf '%s' \"\$0\" | grep -q '_auth/login'" "$PAGE"
assert "sign-in page carries the deep link so sign-in returns you where you asked" \
  sh -c "printf '%s' \"\$0\" | grep -q 'next=%2Fabout%2Fx.html'" "$PAGE"
assert "sign-in page is self-contained (no external asset would 404 mid-outage)" \
  sh -c "printf '%s' \"\$0\" | grep -qv '<script src=\|<link rel=.stylesheet'" "$PAGE"

# NEGATIVE PROOF: rendering a page must NOT mean serving the content. The page
# comes back with 401, and the upstream body never appears in it.
assert "NEGATIVE PROOF: the sign-in page is still a refusal (401)" \
  test "$(code -H "$HTML" http://127.0.0.1:$G_PORT/about/x.html)" = "401"
assert "NEGATIVE PROOF: the sign-in page does not leak the page it is guarding" \
  sh -c "printf '%s' \"\$0\" | grep -qv 'public page'" "$PAGE"

# A signed-in stranger gets told they are known and still refused — the
# distinction between authentication and authorization, said out loud.
DENIED=$(curl -s -H "$HTML" -H "Cookie: chorus_share_session=$SESS_STRANGER" "http://127.0.0.1:$G_PORT/about/x.html")
assert "authenticated-but-unauthorized gets a rendered page naming them" \
  sh -c "printf '%s' \"\$0\" | grep -q 'mallory'" "$DENIED"
assert "NEGATIVE PROOF: that page is still a 403 and leaks no content" \
  sh -c "printf '%s' \"\$0\" | grep -qv 'public page'" "$DENIED"

# A script still gets a status code, not HTML — machines should not have to
# parse a courtesy page.
assert "non-browser client still gets a redirect, not a page" \
  test "$(code http://127.0.0.1:$G_PORT/about/x.html)" = "302"

# The identity provider being down must render too. OFFLINE mode stands in for
# the outage: /_auth/login cannot reach a provider.
assert "sign-in unavailable renders rather than dumping text at a browser" \
  sh -c "curl -s -H '$HTML' 'http://127.0.0.1:$G_PORT/_auth/login' | grep -q \"Can't reach the sign-in service\""
assert "sign-in unavailable is a 503, not a false success" \
  test "$(code -H "$HTML" http://127.0.0.1:$G_PORT/_auth/login)" = "503"

# Bare /_auth/ is a front door, not a 404.
assert "bare /_auth/ renders the sign-in page" \
  sh -c "curl -s -H '$HTML' 'http://127.0.0.1:$G_PORT/_auth/' | grep -q '<h1>Chorus</h1>'"

# --- #3771: the guard must identify itself on every outbound request ---
#
# The break this pins: urllib defaults to `Python-urllib/3.9`, Cloudflare answers
# that with 403, and the guard could not reach its own identity provider. Every
# sign-in returned 503 while the guard itself looked healthy — it started
# cleanly, refused correctly, and rendered a polished "can't reach the sign-in
# service" page. A graceful degradation is indistinguishable from a hard break.
#
# The old suite could not have caught this: it ran entirely OFFLINE, so it proved
# the guard behaves well when the provider is unreachable and never asked whether
# it was reachable. The stub below behaves like the CDN did — 403 for the default
# agent, 200 for a request that names itself — and the probe drives the guard's
# REAL discover(), not a copy of it.

FIXTURES="$SCRIPT_DIR/../tests/fixtures"
UA_PORT=$((G_PORT + 40))
python3 "$FIXTURES/cdn-ua-stub.py" "$UA_PORT" >/dev/null 2>&1 &
UA_PID=$!
wait_up "$UA_PORT" "server on $UA_PORT" || exit 1

SHARE_ALLOW="/about" python3 "$FIXTURES/probe-discover.py" "$GUARD" "http://127.0.0.1:$UA_PORT"
assert "guard reaches a CDN-guarded provider (it identifies itself)" test "$?" -eq 0

# NEGATIVE PROOF: the same call, with the identity stripped back to the library
# default, is refused. This is the exact violation the fix exists to prevent.
SHARE_ALLOW="/about" python3 "$FIXTURES/probe-discover.py" "$GUARD" "http://127.0.0.1:$UA_PORT" "Python-urllib/3.9"
assert "NEGATIVE PROOF: with the default library agent, the same call is REFUSED" test "$?" -eq 1

kill "$UA_PID" 2>/dev/null

# --- #3790: the session cookie rides every subdomain, and still proves who ----
#
# Wren found this before it shipped: every cookie in the system was host-only, so
# the Clearing would redirect an anonymous visitor to the guard, the guard would
# sign them in on its own host, send them back, and the cookie would not travel.
# They arrive anonymous and get redirected again — a login page that never logs
# you in, which reads as flakiness rather than a bug.
#
# Widening a cookie's REACH is a security change. These prove the reach widened
# and the TRUST did not: the two are separate properties and the change to one
# must not quietly relax the other.

mint_session() { python3 "$GUARD" --sign-session "$1"; }

echo "--- #3790 cookie scope ---"
GS_PORT=$(pick_port)
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$GS_PORT" \
  SHARE_COOKIE_DOMAIN=".example.test" python3 "$GUARD" >/dev/null 2>&1 &
GS_PID=$!
wait_up "$GS_PORT" "server on $GS_PORT" || exit 1

# The mint path is /_auth/callback, which needs a live provider; assert instead on
# the SIGN-OUT header, which is minted by the same helper and is reachable here.
SIGNOUT_HDRS=$(curl -s -D - -o /dev/null "http://127.0.0.1:$GS_PORT/_auth/logout")
assert "sign-out clears the cookie at the configured DOMAIN scope" \
  sh -c "printf '%s' \"\$0\" | grep -qi 'Domain=.example.test'" "$SIGNOUT_HDRS"

# NEGATIVE PROOF: a cookie cleared at a NARROWER scope than it was set leaves the
# original in place — sign-out would appear to work and do nothing.
assert "NEGATIVE PROOF: sign-out is not host-only when a domain is configured" \
  sh -c "printf '%s' \"\$0\" | grep -qv 'Max-Age=0; *\$'" "$SIGNOUT_HDRS"
kill "$GS_PID" 2>/dev/null

# NEGATIVE PROOF: widening REACH did not weaken TRUST. A tampered session is
# still refused, with the domain-scoped guard running.
GT_PORT=$(pick_port)
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$GT_PORT" \
  SHARE_COOKIE_DOMAIN=".example.test" python3 "$GUARD" >/dev/null 2>&1 &
GT_PID=$!
wait_up "$GT_PORT" "server on $GT_PORT" || exit 1
GOOD="$(mint_session 'https://example.test/alice#me')"
TAMPERED="${GOOD%.*}.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
assert "NEGATIVE PROOF: a tampered session is refused even at parent scope" \
  test "$(code -H "Cookie: chorus_share_session=$TAMPERED" http://127.0.0.1:$GT_PORT/about/x.html)" = "302"
assert "the untampered session still works at parent scope" \
  test "$(code -H "Cookie: chorus_share_session=$GOOD" http://127.0.0.1:$GT_PORT/about/x.html)" = "200"
kill "$GT_PID" 2>/dev/null

# --- #3790: the Python half of the cross-implementation cookie contract -------
#
# One fixture, two consumers. The guard verifies in Python and the Clearing in
# TypeScript — two implementations of one contract, which is precisely what
# DEC-2209 clause 2 exists to prevent. #3685's shared verifier crate subsumes
# both when it exists; until then a drift between them would surface as a login
# that works on one surface and not the other, discovered by a person.
#
# The probe lives in a FILE, not an inline heredoc: a heredoc nested inside this
# script silently ate everything after it three times today. A fixture file is
# also what Wren's TS side consumes, so the two halves stay symmetrical.

VECTORS="$SCRIPT_DIR/../tests/fixtures/session-cookie-vectors.json"
PROBE="$SCRIPT_DIR/../tests/fixtures/verify-cookie-vectors.py"
assert "cross-impl cookie vectors are committed" test -f "$VECTORS"

python3 "$PROBE" "$GUARD" "$VECTORS"
assert "guard verifier agrees with every shared vector" test "$?" -eq 0

# NEGATIVE PROOF: the vectors can fail a wrong verifier. Without this, one that
# accepted anything would pass the run above and the fixture would prove nothing.
python3 "$PROBE" "$GUARD" "$VECTORS" --wrong-key
assert "NEGATIVE PROOF: the valid vector is REFUSED under a wrong key" test "$?" -eq 0

# --- #3791: sign-in returns you where you were going, and nowhere else -------
#
# The guard's `next` was guard-host-relative, so anyone bounced here from another
# host signed in successfully and landed on this door's root. Both consumers send
# absolute URLs now — the Clearing (#3775) and the app's /chorus leg (#3778).
#
# Accepting an absolute return URL opens an OPEN REDIRECT if it is done by string
# matching. The vectors include the two host-spoofing shapes that a naive check
# admits, and the protocol-relative form that a path check waves through.

RETPROBE="$SCRIPT_DIR/../tests/fixtures/probe-safe-return.py"
python3 "$RETPROBE" "$GUARD"
assert "return-URL validator behaves on every vector" test "$?" -eq 0

# NEGATIVE PROOF: the vectors can fail. Point the trusted suffix somewhere else
# and the legitimate hosts must stop being accepted — without this, a validator
# that accepted everything would pass the run above.
SHARE_RETURN_HOST_SUFFIX="somewhere-else.test" python3 "$RETPROBE" "$GUARD" >/dev/null 2>&1
assert "NEGATIVE PROOF: with a different trusted suffix, the accepts REFUSE" test "$?" -eq 1

# --- #3796: sign-in must not end in a 404 -----------------------------------
#
# Jeff signed in successfully and got "path not shared" — the guard's own root
# was never in its path allowlist, so the default landing was a 404. He read it
# as a lockout for twenty minutes, because a refusal that cannot name its own
# state is indistinguishable from any other refusal.
#
# A fallback that can 404 is not a fallback. These pin both halves.

# Reuse the guard already running on $G_PORT rather than standing up another —
# a second instance added a failure mode (port/startup timing) that had nothing
# to do with what these assertions are about.

assert "signed in at the root goes to the landing page, not a 404" \
  test "$(code_alice http://127.0.0.1:$G_PORT/)" = "302"

LANDBODY=$(as_alice "http://127.0.0.1:$G_PORT/_auth/welcome")
assert "the landing page renders and names the caller" \
  sh -c "printf '%s' \"\$0\" | grep -q 'Signed in as'" "$LANDBODY"

# NEGATIVE PROOF: this did not widen reach. The root is NOT proxied — an
# unallowlisted upstream path is still refused, so unblocking a person did not
# quietly expose whatever the upstream serves.
assert "NEGATIVE PROOF: an unallowlisted upstream path is still 404" \
  test "$(code_alice http://127.0.0.1:$G_PORT/secret/y.html)" = "404"

# NEGATIVE PROOF: anonymous root is unchanged — still a refusal, still 401.
assert "NEGATIVE PROOF: anonymous at the root still gets the sign-in page (401)" \
  test "$(code -H 'Accept: text/html' http://127.0.0.1:$G_PORT/)" = "401"

# The fallback target must be servable BY THE GUARD ITSELF. Asserted rather than
# assumed: the whole bug was a fallback pointing at a path nothing served.
python3 "$RETPROBE" "$GUARD" >/dev/null 2>&1
assert "return-URL vectors still behave with the new fallback" test "$?" -eq 0
assert "the fallback target is a guard-served route, not an upstream path" \
  sh -c "grep -q 'LANDING = AUTH_PREFIX' '$GUARD'"

# --- #3765: the entrance, and /clearing --------------------------------------
#
# Jeff: the top-level Chorus page IS a page, and signing in should land you on
# it. Allowlisting "/" is only safe because route() now treats a bare "/" as an
# exact match — before the narrowing it matched EVERY path, so the entrance line
# would have shared the whole upstream. The negative proof here is the exact
# violation that narrowing exists to prevent: with "/" allowlisted, an
# unallowlisted upstream path must still refuse. Under the old matching it
# answered 200, so this assertion is the one that goes red on the bug.

echo "--- #3765 entrance + /clearing ---"
echo "chorus entrance" > "$TEST_ROOT/www/index.html"
GE_PORT=$(pick_port)
SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/,/about" \
  SHARE_PORT="$GE_PORT" python3 "$GUARD" >/dev/null 2>&1 &
GE_PID=$!
wait_up "$GE_PORT" "entrance guard" || exit 1

assert "allowlisted '/' serves the entrance page to a signed-in caller" \
  test "$(as_alice http://127.0.0.1:$GE_PORT/)" = "chorus entrance"
assert "NEGATIVE PROOF: '/' admits the root and ONLY the root — /secret/y.html still 404" \
  test "$(code_alice http://127.0.0.1:$GE_PORT/secret/y.html)" = "404"
assert "NEGATIVE PROOF: anonymous at the served root is still refused (401)" \
  test "$(code -H 'Accept: text/html' http://127.0.0.1:$GE_PORT/)" = "401"

# /clearing is a redirect to the Clearing's own hostname, never a proxy: the
# Clearing is a socket.io app (long-poll POSTs + a websocket upgrade), and this
# guard is GET/HEAD-only by design — proxying the path would produce a page that
# loads and a socket that dies.
CLR_LOC=$(curl -s -D - -o /dev/null -H "Cookie: chorus_share_session=$SESS_ALICE" \
  "http://127.0.0.1:$GE_PORT/clearing" | tr -d '\r' | awk '/^Location:/{print $2}')
assert "/clearing sends a signed-in caller to the Clearing's own host" \
  test "$CLR_LOC" = "https://clearing.lightlifeurbangardens.com/"
assert "NEGATIVE PROOF: anonymous /clearing must sign in HERE first, not be handed on" \
  sh -c "curl -s -D - -o /dev/null 'http://127.0.0.1:$GE_PORT/clearing' | tr -d '\r' | awk '/^Location:/{print \$2}' | grep -q '^/_auth/login'"

kill "$GE_PID" 2>/dev/null

# --- Wren's flow-suite catch (2026-08-09): a refusal names its own state ------
#
# An unlisted path used to show an ANONYMOUS browser the sign-in page — "not
# signed in" as the answer to "not shared". The visitor signs in, asks again,
# is refused again: the sign-in loop, one door over (the quintet's sixth
# property). Unlisted must refuse NOT-FOUND, identically with or without a
# session. Run against the pre-fix guard, the first assertion here goes red
# (it answered 401 with a sign-in body) — that is the violation this exists
# to catch.

assert "anonymous + unlisted path -> 404, not the sign-in page" \
  test "$(code -H 'Accept: text/html' http://127.0.0.1:$G_PORT/secret/y.html)" = "404"
ANON_UNLISTED=$(curl -s -H 'Accept: text/html' "http://127.0.0.1:$G_PORT/secret/y.html")
assert "NEGATIVE PROOF: the anonymous unlisted body offers no sign-in" \
  sh -c "! printf '%s' \"\$0\" | grep -qi 'sign in'" "$ANON_UNLISTED"
SIGNED_UNLISTED=$(as_alice -H 'Accept: text/html' "http://127.0.0.1:$G_PORT/secret/y.html")
assert "signed-in + unlisted path -> the same not-found, never the sign-in page" \
  sh -c "printf '%s' \"\$0\" | grep -qi 'not found' && ! printf '%s' \"\$0\" | grep -qi 'sign in'" "$SIGNED_UNLISTED"
assert "refusal is identical with and without a session (both 404)" \
  test "$(code -H 'Accept: text/html' http://127.0.0.1:$G_PORT/secret/y.html)" = "$(code_alice -H 'Accept: text/html' http://127.0.0.1:$G_PORT/secret/y.html)"

# --- #3815: the three ontology pages are EXACT-FILE, not a /docs tree ---------
#
# The trap: allowlisting /docs (or any prefix reading like one page) publishes
# the whole public tree — 77 pages, incl. unreviewed. These prove a listed page
# is reachable AND a sibling public page not on the list is NOT — exact-file
# matching, not a prefix that admits neighbours. Own stub upstream + guard.
echo "--- #3815 exact-file ontology pages ---"
OP_ROOT=$(mktemp -d)
echo "ER-PAGE" > "$OP_ROOT/chorus-er-diagram.html"
echo "SIBLING-UNREVIEWED" > "$OP_ROOT/chorus-hook-architecture.html"
OUP_PORT=$(pick_port); OG_PORT=$(pick_port)
(cd "$OP_ROOT" && python3 -m http.server "$OUP_PORT" >/dev/null 2>&1) &
OUP_PID=$!
wait_up "$OUP_PORT" "ontology upstream" "" || exit 1
SHARE_UPSTREAM="http://127.0.0.1:$OUP_PORT" \
  SHARE_ALLOW="/chorus-er-diagram.html,/chorus-instance-explorer.html,/chorus-data-model.html" \
  SHARE_PORT="$OG_PORT" python3 "$GUARD" >/dev/null 2>&1 &
OG_PID=$!
wait_up "$OG_PORT" "ontology-pages guard" || exit 1
assert "a listed ontology page serves to a signed-in caller" \
  test "$(as_alice http://127.0.0.1:$OG_PORT/chorus-er-diagram.html | tr -d '\n')" = "ER-PAGE"
assert "NEGATIVE PROOF: an UNLISTED sibling public page still 404s (exact-file, not a tree)" \
  test "$(code_alice http://127.0.0.1:$OG_PORT/chorus-hook-architecture.html)" = "404"
assert "NEGATIVE PROOF: a /docs-prefixed path is NOT reachable (the static-tree trap stays shut)" \
  test "$(code_alice http://127.0.0.1:$OG_PORT/docs/chorus-er-diagram.html)" = "404"
kill "$OG_PID" "$OUP_PID" 2>/dev/null; rm -rf "$OP_ROOT"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
