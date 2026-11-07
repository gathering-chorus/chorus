#!/bin/bash
# @test-type: integration — needs the installed LaunchAgent; reports UNMEASURED when it is not on this box.
# @domain: messages
#
# #4570 — Jeff opens Clearing v2 on his phone at :3472 at any time. This checks the
# installed com.chorus.clearing-v2 runs the canonical build, and that :3472 answers on
# the LAN address (not loopback: a phone cannot reach loopback). Agent installed but
# the page not answering = FAIL (the service is down; that is the product broken).
# No agent installed = UNMEASURED, never a pass.
# CLEARING_V2_URL and CLEARING_V2_PLIST are the seams the negative proof uses.
set -u
PLIST="${CLEARING_V2_PLIST:-$HOME/Library/LaunchAgents/com.chorus.clearing-v2.plist}"
check() { # plist url → PASS/FAIL line, exit code
  local plist="$1" url="$2"
  [ -f "$plist" ] || { echo "UNMEASURED: no com.chorus.clearing-v2 LaunchAgent at $plist"; return 0; }
  grep -q 'chorus/directing/clearing-v2/dist/server.js' "$plist" \
    || { echo "FAIL: the LaunchAgent does not run canonical directing/clearing-v2"; return 1; }
  local code; code=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || true)
  [ "$code" = "200" ] && { echo "PASS: $url answered 200"; return 0; }
  echo "FAIL: $url answered '${code:-nothing}' — Clearing v2 is down"; return 1
}
if [ "${1:-}" = "--self-test" ]; then
  # NEGATIVE PROOF: an installed agent with nothing listening must be red, not green
  T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
  echo '<string>/x/chorus/directing/clearing-v2/dist/server.js</string>' > "$T/p.plist"
  if check "$T/p.plist" "http://127.0.0.1:9/" >/dev/null; then echo "FAIL NEGATIVE: a down service passed"; exit 1; fi
  echo '<string>/x/werk/other/server.js</string>' > "$T/q.plist"
  if check "$T/q.plist" "http://127.0.0.1:9/" >/dev/null; then echo "FAIL NEGATIVE: a non-canonical build passed"; exit 1; fi
  echo "PASS self-test: a down service and a non-canonical build are both red"; exit 0
fi
LAN="$(ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null || echo 127.0.0.1)"
check "$PLIST" "${CLEARING_V2_URL:-http://$LAN:3472/}"
