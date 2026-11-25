#!/usr/bin/env bats
# @test-type: integration — the real Caddy binary on a scratch port with the edge Caddyfile, stub upstreams; no tunnel, no live edge
# @domain: security
# @card: #5039 AC3 · owner: silas
#
# On the public apex (lightlifeurbangardens.com) /athena/domains.html answered 404 and /borg/
# answered 200 with no sign-in (measured 2026-11-25 05:19), and so did the data under
# /api/chorus/* (Wren, 05:27: posture, roles, cost, sessions): the edge Caddy on :3000 sends
# Chorus page paths to chorus-api or the garden app with no door. #5039: a request through the
# public tunnel (Cloudflare always sets Cf-Connecting-Ip) on a Chorus page path is sent to the
# signed-in door at /chorus/<path>; localhost and the Wi-Fi (no such header) serve as before.
#   - tunnel: /athena/domains.html, /athena/domain.html?d=principles, /borg/, /loom/, /werk/,
#     /chorus-pages/x.html → 302 /chorus<same path and query>;
#   - local: the same paths reach their upstreams, as before;
#   - the garden site's own paths are untouched on the tunnel;
#   - NEGATIVE: the Caddyfile before #5039 answers the tunnel /athena/domains.html with the
#     upstream (the 404 class), and this check fails on it.
#
# Covers: platform/scripts/launchagents-canonical/gathering-edge-Caddyfile

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
EDGE="$ROOT/platform/scripts/launchagents-canonical/gathering-edge-Caddyfile"
CADDY="${CADDY_BIN:-/opt/homebrew/opt/caddy/bin/caddy}"

port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

# a stub upstream that answers 200 with its own name, so a test can tell who served
stub() {
  python3 -c '
import http.server,sys
name=sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        b=(name+" "+self.path).encode(); self.send_response(200); self.send_header("Content-Length",str(len(b))); self.end_headers(); self.wfile.write(b)
    def log_message(self,*a): pass
http.server.HTTPServer(("127.0.0.1",int(sys.argv[1])),H).serve_forever()' "$1" "$2" &
  echo $! >> "$T/pids"
}

# run Caddy on the given Caddyfile with :3000 and the upstream ports moved to scratch ones
start() {
  EP="$(port)"; API="$(port)"; APP="$(port)"
  stub "$API" chorus-api; stub "$APP" garden-app
  sed -e "s/^:3000 {/:$EP {/" -e "s/admin localhost:2019/admin off/" \
      -e "s/localhost:3340/127.0.0.1:$API/g" -e "s/localhost:3002/127.0.0.1:$APP/g" \
      -e '/^	log {/,/^	}/d' \
      -e '/^:3520 {/,$d' "$1" > "$T/Caddyfile"
  "$CADDY" run --config "$T/Caddyfile" --adapter caddyfile > "$T/caddy.log" 2>&1 &
  echo $! >> "$T/pids"
  for _ in $(seq 1 100); do curl -s -o /dev/null "http://127.0.0.1:$EP/" && return 0; sleep 0.1; done
  cat "$T/caddy.log"; return 1
}

# status and Location for one request; $2 = tunnel|local
hit() {
  if [ "$2" = tunnel ]; then
    curl -s -o /dev/null -w '%{http_code} %{redirect_url}' -H 'Cf-Connecting-Ip: 203.0.113.7' "http://127.0.0.1:$EP$1"
  else
    curl -s -o /dev/null -w '%{http_code} %{redirect_url}' "http://127.0.0.1:$EP$1"
  fi
}

setup() {
  test -x "$CADDY" || skip "caddy not installed at $CADDY"
  T="$(mktemp -d)"; : > "$T/pids"
}
teardown() { [ -f "$T/pids" ] && kill $(cat "$T/pids") 2>/dev/null; rm -rf "$T"; }

@test "#5039 on the tunnel every Chorus page path goes to the signed-in door at /chorus" {
  start "$EDGE"
  for p in /athena/domains.html '/athena/domain.html?d=principles' /borg/ /loom/ /werk/ /chorus-pages/x.html /borg /domains/x.html /api/chorus/context/roles '/api/chorus/x?y=1'; do
    out="$(hit "$p" tunnel)"
    test "$out" = "302 http://127.0.0.1:$EP/chorus$p" || { echo "tunnel $p → $out"; return 1; }
  done
}

@test "#5039 on localhost and the Wi-Fi the same paths serve as before" {
  start "$EDGE"
  test "$(hit /borg/ local)" = "200 " || { echo "local /borg/ → $(hit /borg/ local)"; return 1; }
  test "$(hit /athena/domains.html local)" = "200 " || { echo "local /athena → $(hit /athena/domains.html local)"; return 1; }
  test "$(hit /borg local)" = "308 http://127.0.0.1:$EP/borg/" || { echo "local /borg → $(hit /borg local)"; return 1; }
  test "$(hit /api/chorus/context/roles local)" = "200 " || { echo "local /api/chorus → $(hit /api/chorus/context/roles local)"; return 1; }
}

@test "#5039 the garden site's own paths are untouched on the tunnel" {
  start "$EDGE"
  for p in / /about /garden/beds.html; do
    out="$(hit "$p" tunnel)"
    test "$out" = "200 " || { echo "tunnel $p → $out"; return 1; }
  done
}

@test "#5039 NEGATIVE: the Caddyfile before #5039 serves the tunnel /athena page with no door, and the check fails on it" {
  # the edge as it was: the #5039 block cut out (the edit must have happened, or this proves nothing)
  sed '/#5039/,/redir @chorus_public/d' "$EDGE" > "$T/before"
  ! grep -q chorus_public "$T/before" || { echo "fixture still has the door block"; return 1; }
  start "$T/before"
  out="$(hit /athena/domains.html tunnel)"
  test "$out" != "302 http://127.0.0.1:$EP/chorus/athena/domains.html" || { echo "the pre-fix edge already redirects: $out"; return 1; }
  out="$(hit /borg/ tunnel)"
  test "$out" = "200 " || { echo "expected the old anonymous 200, got $out"; return 1; }
  out="$(hit /api/chorus/context/roles tunnel)"
  test "$out" = "200 " || { echo "expected the old anonymous data 200, got $out"; return 1; }
}
