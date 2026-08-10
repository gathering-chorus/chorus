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

echo "=== prefix suite: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
