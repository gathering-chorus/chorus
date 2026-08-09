#!/usr/bin/env bash
# SHARE_PATH_PREFIX — the guard mounted under a path, not at a host root. (#3765)
#
# Jeff's four-URL scheme: one host, and every Chorus surface a child of /chorus.
# cloudflared matches a path but does not rewrite it, so the guard is handed
# /chorus/athena while the service behind it serves /athena.
#
# Each assertion below is paired with the state it must be able to see. The
# guard's own suite already covers prefix-unset; this file covers prefix-set,
# and every check names what would break if it silently stopped applying.
set -u

GUARD="$(cd "$(dirname "$0")" && pwd)/chorus-share-guard.py"
ROOT="$(mktemp -d)"; trap 'rm -rf "$ROOT"' EXIT
mkdir -p "$ROOT/www/athena"
echo "ATHENA-PAGE" > "$ROOT/www/athena/index.html"
echo "ENTRANCE-PAGE" > "$ROOT/www/index.html"

UP=$((26000 + ($$ % 9000))); GP=$((UP + 1))
(cd "$ROOT/www" && python3 -m http.server "$UP" >/dev/null 2>&1) &
ME="https://example.test/alice#me"
printf '%s\n' "$ME" > "$ROOT/p.txt"
printf '/\n/athena\n' > "$ROOT/allow.txt"
export SHARE_PRINCIPALS_FILE="$ROOT/p.txt" SHARE_STATE_FILE="$ROOT/s.json" SHARE_OIDC_OFFLINE=1

SHARE_PATH_PREFIX="/chorus" SHARE_ALLOW_FILE="$ROOT/allow.txt" \
  SHARE_UPSTREAM="http://127.0.0.1:$UP" SHARE_PORT="$GP" python3 "$GUARD" >/dev/null 2>&1 &
for _ in $(seq 1 25); do curl -s -o /dev/null "http://127.0.0.1:$GP/_auth/" 2>/dev/null && break; sleep 0.3; done

S=$(SHARE_STATE_FILE="$ROOT/s.json" python3 "$GUARD" --sign-session "$ME" 2>/dev/null)
B="http://127.0.0.1:$GP"
get()  { curl -s --max-time 10 -H "Cookie: chorus_share_session=$S" "$B$1"; }
code() { curl -s -o /dev/null --max-time 10 -w '%{http_code}' -H "Cookie: chorus_share_session=$S" "$B$1"; }

pass=0; fail=0
ok()  { if [ "$2" = "$3" ]; then echo "PASS: $1"; pass=$((pass+1));
        else echo "FAIL: $1 (got '$2', want '$3')"; fail=$((fail+1)); fi; }

# The entrance, under the prefix.
ok "/chorus serves the entrance"            "$(get /chorus | tr -d '\n')"        "ENTRANCE-PAGE"
ok "/chorus/ serves the entrance too"        "$(get /chorus/ | tr -d '\n')"       "ENTRANCE-PAGE"
ok "/chorus/athena/ serves the child page"   "$(get /chorus/athena/ | tr -d '\n')" "ATHENA-PAGE"

# NEGATIVE PROOF for the strip itself. If the guard stopped stripping, the paths
# above would 404 — but so would a guard that was simply broken. The check that
# separates those two: the UNPREFIXED path must NOT be served, because a guard
# mounted under /chorus is not reachable at the host root, and one that answers
# both is not stripping, it is ignoring the prefix.
ok "unprefixed /athena/ is NOT served under a prefix" "$(code /athena/)"          "404"
ok "unprefixed / is NOT served under a prefix"        "$(code /)"                 "404"

# A path that merely starts with the same letters is not the prefix.
ok "/chorusX is not treated as the prefix"   "$(code /chorusX/athena/)"           "404"

# The sign-in door must be reachable under the prefix, or nobody behind it can
# ever authenticate — the failure would present as "the door doesn't exist".
ok "/chorus/_auth/ is the sign-in page"      "$(curl -s -o /dev/null --max-time 10 -w '%{http_code}' -H 'Accept: text/html' "$B/chorus/_auth/")" "401"

# --- #3805: guard-authored URLs speak the visitor's frame -------------------
#
# Wren's flows caught sign-in from /chorus delivering the garden site's 404:
# every self-referential URL the guard emitted was written in the STRIPPED
# frame, so the browser left the mount. These pin the emissions; the guard's
# main suite pins the unmounted frame (unprefixed URLs), which together form
# the negative proof that pub() is frame-aware, not a blanket prepend.

# A LISTED path, no session, non-HTML: the script-shaped caller gets a 302 to
# sign-in. (An UNLISTED path 404s before auth — that is #3765's refusal fix, so
# it cannot be the probe here.)
LOC=$(curl -s -D - -o /dev/null --max-time 10 "$B/chorus/athena/" | tr -d '\r' | awk '/^Location:/{print $2}')
ok "anonymous redirect stays under the mount" \
   "$(printf '%s' "$LOC" | grep -c '^/chorus/_auth/login?next=%2Fchorus')" "1"

SIGNIN=$(curl -s --max-time 10 -H 'Accept: text/html' "$B/chorus/")
ok "sign-in link on the page carries the mount"      "$(printf '%s' "$SIGNIN" | grep -c 'href="/chorus/_auth/login')" "1"
ok "sign-in next value is the PUBLIC path"           "$(printf '%s' "$SIGNIN" | grep -c 'next=%2Fchorus')" "1"

# Browser-provided next values are already public-frame: they must pass through
# UNTOUCHED. A blanket prepend would emit /chorus/chorus here — the double-mount
# this assertion exists to catch.
DOUBLE=$(curl -s --max-time 10 "$B/chorus/_auth/?next=%2Fchorus%2Fathena%2F")
ok "browser-provided next is NOT re-prefixed"        "$(printf '%s' "$DOUBLE" | grep -c 'next=%2Fchorus%2Fchorus')" "0"
ok "browser-provided next passes through intact"     "$(printf '%s' "$DOUBLE" | grep -c 'next=%2Fchorus%2Fathena')" "1"

LOGOUT=$(curl -s -D - -o /dev/null --max-time 10 -H "Cookie: chorus_share_session=$S" "$B/chorus/_auth/logout" | tr -d '\r' | awk '/^Location:/{print $2}')
ok "sign-out lands back under the mount"             "$LOGOUT" "/chorus/"

WELCOME=$(curl -s --max-time 10 -H "Cookie: chorus_share_session=$S" "$B/chorus/_auth/welcome")
ok "welcome links carry the mount"                   "$(printf '%s' "$WELCOME" | grep -c 'href="/chorus/athena/model.html"')" "1"
ok "welcome sign-out carries the mount"              "$(printf '%s' "$WELCOME" | grep -c 'href="/chorus/_auth/logout"')" "1"

echo "=== prefix suite: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
