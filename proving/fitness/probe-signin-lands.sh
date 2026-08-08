#!/usr/bin/env bash
# ROW 2 — a sign-in lands somewhere real. (#3765)
#
# WHY THIS ROW EXISTS
#
# Three separate times the sign-in itself worked perfectly and the person was
# still stuck:
#   · sign-in succeeded and handed Jeff a 404, for twenty minutes (#3796)
#   · a verified caller who was not on the list got redirected to sign in,
#     which succeeded, which redirected again — eight rounds before the
#     browser gave up (#3770)
#   · an anonymous browser was bounced straight to the identity provider, so
#     the first thing a visitor saw was someone else's domain asking for
#     credentials with no indication of what asked or why
#
# Every one of those is a working authentication and a broken arrival. So this
# probe never asserts "the sign-in worked" — it asserts WHERE A PERSON ENDS UP.
#
# HOW THE RED STATE IS PROVEN REACHABLE
#
# --prove-red boots a MUTATED COPY of the guard with one specific fix deleted,
# and requires the probe to go red against it. Not a fixture of what a failure
# might look like — the actual prior code, running. A check that has only ever
# been run against the fixed version cannot be said to catch anything.
set -u
# Deliberately NOT pipefail. `printf ... | grep -q` returns grep's status, but
# grep -q exits the instant it matches, which breaks the pipe under printf and
# makes pipefail report the pipeline as failed — a MATCH reads as a MISS. That
# cost an hour here: assertion C below fired against a perfectly healthy guard.
# Matching is done with herestrings instead, which have no pipe to break.

GUARD="${GUARD_PATH:-$(cd "$(dirname "$0")/../.." && pwd)/platform/scripts/chorus-share-guard.py}"
PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

[ -f "$GUARD" ] || { echo "row2 UNMEASURABLE — guard not found at $GUARD" >&2; exit 2; }

ROOT="$(mktemp -d)"; trap 'kill ${G_PID:-0} ${UP_PID:-0} 2>/dev/null; rm -rf "$ROOT"' EXIT
mkdir -p "$ROOT/www/about"; echo "<html><body>upstream about page</body></html>" > "$ROOT/www/about/x.html"

# ports: fixed offsets from the pid so parallel runs don't collide
UP_PORT=$((20000 + ($$ % 9000))); G_PORT=$((UP_PORT + 1))
(cd "$ROOT/www" && python3 -m http.server "$UP_PORT" >/dev/null 2>&1) & UP_PID=$!

ME="https://example.test/alice#me"
STRANGER="https://example.test/mallory#me"
printf '%s\n' "$ME" > "$ROOT/principals.txt"
export SHARE_PRINCIPALS_FILE="$ROOT/principals.txt" SHARE_STATE_FILE="$ROOT/state.json" SHARE_OIDC_OFFLINE=1

RUN="$GUARD"
if [ "$PROVE_RED" -eq 1 ]; then
  # Delete the #3796 root special-case — the exact three lines whose absence
  # sent a successfully signed-in Jeff to a 404. Everything else is untouched,
  # so a red here can only be attributed to that.
  RUN="$ROOT/guard-pre-3796.py"
  python3 - "$GUARD" "$RUN" <<'PY'
import sys, re
src = open(sys.argv[1]).read()
# The #3796 guarantee moved when the root became servable (#3765): the redirect
# is now conditional on the root NOT being served. Same guarantee, later line.
# The old pattern stopped matching and this proof REFUSED rather than passing —
# which is the whole reason it was written to refuse.
out = re.sub(
    r'\n *if upstream is None and self\.path\.split\("\?"\)\[0\] in \("/", ""\):\n *return self\._redirect\(LANDING\)\n',
    '\n', src, count=1)
if out == src:
    sys.stderr.write("row2: could not remove the #3796 root case — the guard changed shape.\n"
                     "A negative proof that silently mutates nothing proves nothing.\n")
    sys.exit(3)
open(sys.argv[2], "w").write(out)
PY
  [ $? -eq 0 ] || { echo "row2 UNMEASURABLE — negative-proof mutation failed" >&2; exit 2; }
fi

SHARE_UPSTREAM="http://127.0.0.1:$UP_PORT" SHARE_ALLOW="/about" SHARE_PORT="$G_PORT" \
  python3 "$RUN" >/dev/null 2>&1 & G_PID=$!
for _ in $(seq 1 25); do curl -s -o /dev/null "http://127.0.0.1:$G_PORT/_auth/" 2>/dev/null && break; sleep 0.3; done

# Sessions come from the guard's OWN signing code. A second implementation
# would agree with the first while both were wrong.
SESS=$(python3 "$RUN" --sign-session "$ME" 2>/dev/null)
SESS_X=$(python3 "$RUN" --sign-session "$STRANGER" 2>/dev/null)
[ -n "$SESS" ] || { echo "row2 UNMEASURABLE — guard would not mint a session" >&2; exit 2; }

B="http://127.0.0.1:$G_PORT"
walk()  { curl -s -L --max-redirs 5 -H "Cookie: chorus_share_session=$1" "$B$2"; }
final() { curl -s -o /dev/null -L --max-redirs 5 -w "%{http_code}" -H "Cookie: chorus_share_session=$1" "$B$2"; }
hops()  { curl -s -o /dev/null -L --max-redirs 8 -w "%{num_redirects}" -H "Cookie: chorus_share_session=$1" "$B$2"; }

fail=""

# A · an admitted person opening the front door arrives at a page that tells
#     them they are in. Following redirects, because a 302 is not an arrival.
code=$(final "$SESS" "/"); body=$(walk "$SESS" "/")
if [ "$code" != "200" ]; then
  fail="$fail an admitted person opening / ends at HTTP $code, not a page;"
elif ! printf '%s' "$body" | grep -qi 'signed in as'; then
  fail="$fail / returns 200 but the page never says who is signed in;"
fi

# B · a verified caller who is NOT admitted gets a terminal answer. The failure
#     was not the refusal — it was answering a refusal with a redirect, which
#     the identity provider satisfied, forever.
n=$(hops "$SESS_X" "/about/x.html"); code_x=$(final "$SESS_X" "/about/x.html")
if [ "${n:-0}" -gt 1 ]; then
  fail="$fail a signed-in-but-unlisted caller is bounced ${n} times instead of refused;"
elif [ "$code_x" != "403" ]; then
  fail="$fail a signed-in-but-unlisted caller gets HTTP $code_x, not a refusal naming them;"
fi

# C · an anonymous BROWSER is told where it is before being sent anywhere.
#     Landing on a stranger's credential prompt with no context is the shape of
#     a phishing page, whether or not it is one.
anon=$(curl -s -H 'Accept: text/html' "$B/about/x.html")
if ! grep -qi 'chorus' <<<"$anon"; then
  fail="$fail an anonymous browser is not shown our own sign-in page first;"
fi

if [ -n "$fail" ]; then
  echo "row2 RED — the sign-in works and the arrival does not:$fail"
  [ "$PROVE_RED" -eq 1 ] && { echo "NEGATIVE PROOF OK — with the #3796 fix removed, the probe goes red."; exit 0; }
  exit 1
fi

if [ "$PROVE_RED" -eq 1 ]; then
  echo "NEGATIVE PROOF FAILED — the guard with the #3796 fix REMOVED still passed." >&2
  echo "The probe cannot separate the two states it exists to separate." >&2
  exit 1
fi

echo "row2 GREEN — admitted lands on a page, unlisted gets a terminal refusal, anonymous sees our door"
