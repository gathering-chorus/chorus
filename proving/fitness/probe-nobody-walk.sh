#!/usr/bin/env bash
# ROW 7 — the walk as a nobody, through the real door. (#3765)
#
# WHY THIS ROW EXISTS
#
# "Works on my computer" is literally true here. One machine, one issuer, one
# set of lists, all holding Jeff. Every authenticated test in three repos gets
# its session from a test-only route that mints one for any WebID you name, and
# the single real login spec is skipped with a TODO. So the entire end-to-end
# tier proves the backdoor works.
#
# Both of 2026-08-08's failures were this shape: Jeff signed in fine, because he
# is on the list. The ordinary path was broken in the same minute, same code.
# The privileged path working is not evidence about the unprivileged one, and
# the only way to see the ordinary path is to have no standing at all.
#
# WHAT THIS PROBE REFUSES TO DO
#
# It does not mint a session. It does not call a test route. It does not connect
# from somewhere trusted. Every one of those turns the walk back into the
# developer's walk, which is the thing being measured against.
#
# WHAT IT CANNOT MEASURE, SAID OUT LOUD
#
# Completing a real sign-in as a least-privileged person needs a permanent
# low-privilege account, and none exists. That leg is reported UNMEASURABLE by
# name. A row that quietly drops what it cannot see is worse than one that
# admits it, because the gap stops being visible the moment the line goes green.
set -u

HERE="$(cd "$(dirname "$0")/../.." && pwd)"
PLIST="$HERE/config/launchagents/com.chorus.share-guard.plist"

# Origins are RESOLVED FROM CONFIG, never named here. Wren's discipline, and it
# earned itself immediately: the first version of this probe assumed the public
# root and the governed surface were the same host, so it walked the public
# garden site's /about, got a 200, and reported governed content served to a
# stranger. Naming a hostname in a probe bakes today's topology into a measure
# that is supposed to outlive it — and Jeff is collapsing three hosts into one.
# The name is a <key>, the value is the <string> after it. Matching <string>
# for the name found nothing, ORIGIN came out empty, and every path reported
# "did not answer" — a probe silently measuring nowhere. So resolution refuses
# rather than proceeding when it comes back blank.
plistval() { grep -A1 "<key>$1</key>" "$PLIST" 2>/dev/null | \
             sed -n 's/.*<string>\(.*\)<\/string>.*/\1/p' | head -1; }
GOVERNED="${FITNESS_GOVERNED_ORIGIN:-$(plistval SHARE_PUBLIC_ORIGIN)}"
[ -n "$GOVERNED" ] || { echo "row7 UNMEASURABLE — could not resolve the governed origin from $PLIST" >&2; exit 2; }

# The public site is the governed origin with its leading label removed. Derived,
# not named — the day these collapse to one host with paths, this resolves to the
# same origin and the entry-point leg below becomes trivially satisfied rather
# than wrong.
#
# The first version read a SHARE_RETURN_HOST_SUFFIX key that is not in the plist,
# so SUFFIX was empty, the entry-point leg was SKIPPED, and the row printed GREEN
# while the defect it was written to catch sat right there. A guard whose target
# is missing must fail loudly, never pass vacuously — my own words, and I did it
# anyway within the hour.
PUBLIC="${FITNESS_PUBLIC_ORIGIN:-$(sed -E 's#^(https?://)[^./]+\.#\1#' <<<"$GOVERNED")}"
[ -n "$PUBLIC" ] && [ "$PUBLIC" != "$GOVERNED" ] || PUBLIC=""
ORIGIN="$GOVERNED"
PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

if [ "$PROVE_RED" -eq 1 ]; then
  # The violation, made reachable: a guard with the session check removed, so
  # governed paths serve their content to a caller holding nothing. That is
  # precisely "a nobody reaches an authenticated surface", and it is what the
  # walk below has to be able to see.
  GUARD="$HERE/platform/scripts/chorus-share-guard.py"
  ROOT="$(mktemp -d)"; trap 'rm -rf "$ROOT"' EXIT
  mkdir -p "$ROOT/www/about"; echo "<html><body>governed content</body></html>" > "$ROOT/www/about/x.html"
  MUT="$ROOT/guard-no-session-check.py"
  python3 - "$GUARD" "$MUT" <<'PY' || { echo "row7 UNMEASURABLE — could not build the violation" >&2; exit 2; }
import sys, re
src = open(sys.argv[1]).read()
# Make _session_webid always return an admitted identity: the guard then treats
# every anonymous caller as signed in AND listed.
out, n = re.subn(r'(    def _session_webid\(self\):\n)',
                 r'\1        return "https://fitness.test/nobody#me"\n', src, count=1)
if n != 1:
    sys.stderr.write("row7: could not neuter the session check — the guard changed shape.\n")
    sys.exit(3)
out = out.replace("if webid not in current_principals():",
                  "if False and webid not in current_principals():", 1)
open(sys.argv[2], "w").write(out)
PY
  UP=$((25000 + ($$ % 9000))); GP=$((UP + 1))
  (cd "$ROOT/www" && python3 -m http.server "$UP" >/dev/null 2>&1) &
  printf 'https://fitness.test/nobody#me\n' > "$ROOT/p.txt"
  export SHARE_PRINCIPALS_FILE="$ROOT/p.txt" SHARE_STATE_FILE="$ROOT/s.json" SHARE_OIDC_OFFLINE=1
  SHARE_UPSTREAM="http://127.0.0.1:$UP" SHARE_ALLOW="/about" SHARE_PORT="$GP" \
    python3 "$MUT" >/dev/null 2>&1 &
  for _ in $(seq 1 25); do curl -s -o /dev/null "http://127.0.0.1:$GP/_auth/" 2>/dev/null && break; sleep 0.3; done
  ORIGIN="http://127.0.0.1:$GP"
  PATHS="/about/x.html"
else
  # The real governed surface, read from the list that governs it rather than
  # from a copy in this file — a hardcoded list drifts and then measures nothing.
  PATHS=$(grep -vE '^\s*#|^\s*$' "$HERE/config/share-allowlist.txt" 2>/dev/null | awk '{print $1}')
  [ -n "$PATHS" ] || { echo "row7 UNMEASURABLE — could not read the governed path list" >&2; exit 2; }
fi

# A nobody: no cookie jar, no headers we would not have, a browser's Accept.
nobody() { curl -s --max-time 25 -H 'Accept: text/html' "$@"; }
code()   { curl -s -o /dev/null --max-time 25 -w '%{http_code}' -H 'Accept: text/html' "$@"; }
hops()   { curl -s -o /dev/null --max-time 30 -L --max-redirs 6 -w '%{num_redirects}' -H 'Accept: text/html' "$@"; }

reachable=0
fail=""

for p in $PATHS; do
  c=$(code "$ORIGIN$p")
  case "$c" in
    000) echo "row7 UNMEASURABLE — $ORIGIN$p did not answer. Could not ask." >&2; exit 2 ;;
    200) reachable=$((reachable + 1))
         fail="$fail $p is served to a caller with no standing;" ;;
    401|403|302) ;;
    5*)  fail="$fail $p answers HTTP $c — a stranger is meeting an error, not a door;" ;;
    *)   fail="$fail $p answers HTTP $c, which is neither content nor a coherent refusal;" ;;
  esac
  # A refusal has to END. The eight-round loop on 2026-08-08 was every hop
  # individually correct and the journey non-terminating.
  n=$(hops "$ORIGIN$p")
  [ "${n:-0}" -gt 2 ] && fail="$fail $p bounces a stranger ${n} times instead of answering;"
done

if [ "$PROVE_RED" -eq 0 ]; then
  # No test seam may be reachable from outside. This is the generalisation of
  # the /test/login check: the question is not whether that one route is gated,
  # it is whether ANY way in exists that is not the real door.
  for seam in /test/login /test/clear-visibility-cache; do
    c=$(code -X POST "$ORIGIN$seam")
    case "$c" in
      200|400) fail="$fail a test-only route ($seam) answers publicly with HTTP $c;" ;;
    esac
  done

  # The refusal has to be usable by a person who has never seen this system.
  door=$(nobody "$ORIGIN/athena")
  grep -qi 'sign in' <<<"$door" || fail="$fail the refusal never offers a way in;"

  # THE ENTRY POINT MUST NOT CHANGE THE ANSWER.
  #
  # Asked directly, the governed surface hands a stranger our own sign-in page.
  # Followed from the public site, the same request is bounced through to the
  # identity provider's account page — a different domain asking for credentials
  # with no indication of what asked or why, which is the shape of a phishing
  # page whether or not it is one. The guard's code contains a deliberate
  # comment refusing to do exactly this; the routing in front of it does it
  # anyway, and nobody on the list ever sees it because we all have sessions.
  #
  # So: walk in from outside and require the SAME door.
  if [ -z "$PUBLIC" ]; then
    echo "row7 UNMEASURABLE — could not derive the public entry point; refusing to skip the leg." >&2
    exit 2
  fi
  landed=$(curl -s -L --max-redirs 6 --max-time 30 -H 'Accept: text/html' \
           -o /dev/null -w '%{url_effective}' "$PUBLIC/athena" 2>/dev/null)
  case "$landed" in
    "$GOVERNED"*) ;;
    "")  fail="$fail the public entry point did not answer;" ;;
    *)   fail="$fail entering from the public site, a stranger is handed to $(sed -E 's#(https?://[^/]+).*#\1#' <<<"$landed") instead of our own sign-in page;" ;;
  esac
fi

if [ -n "$fail" ]; then
  echo "row7 RED — a caller with no standing does not get a coherent journey:$fail"
  [ "$PROVE_RED" -eq 1 ] && { echo "NEGATIVE PROOF OK — against a guard whose session check is removed, the walk sees the reach."; exit 0; }
  exit 1
fi

if [ "$PROVE_RED" -eq 1 ]; then
  echo "NEGATIVE PROOF FAILED — governed content was served to a caller holding nothing and the walk passed." >&2
  echo "It cannot see the only thing it exists to see." >&2
  exit 1
fi

echo "row7 GREEN — a caller with no standing is refused coherently and terminally on every governed path,"
echo "         no test seam answers publicly, and the refusal offers a way in."
echo "         NOT PROVEN: that such a person can then actually GET IN. Completing a real sign-in"
echo "         needs a permanent least-privileged account, which does not exist. Until one does,"
echo "         the door is proven to close and is NOT proven to open for anyone but us."
