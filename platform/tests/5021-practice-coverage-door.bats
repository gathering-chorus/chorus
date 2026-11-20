#!/usr/bin/env bats
# @test-type: contract — practice-coverage.sh against a stub principles door and a stub store; no live services.
# @domain: principles
# @card: #5021 · owner: kade
# What Jeff sees: the practice-coverage report reads the principles through their one door
# (principles-api via chorus-api), not from the principles store, so the principles lint has no
# side door left.
#
# platform/scripts/practice-coverage.sh asks the door for the principles and joins them to the
# practices with a VALUES block. NEGATIVE: a door that does not answer is exit 2 ("refusing to
# report a blind zero"), never a clean zero; the script names no principles graph.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PC="$ROOT/platform/scripts/practice-coverage.sh"

setup() {
  T="$BATS_TEST_TMPDIR"
  port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
  cat > "$T/stub.py" <<'PY'
import http.server, sys, urllib.parse
log = sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        if u.path == "/principles":
            b, ct = b'{"data": [{"name": "hemenway-observe"}, {"name": "hemenway-catch-and-store"}]}', "application/json"
        else:  # the store: record the query, answer an empty CSV result with its header
            q = urllib.parse.parse_qs(u.query).get("query", [""])[0]
            open(log, "a").write(q.replace("\n", " ") + "\n")
            b, ct = b"p\n", "text/csv"
        self.send_response(200); self.send_header("Content-Type", ct); self.end_headers(); self.wfile.write(b)
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
  python3 "$T/stub.py" "$port" "$T/queries.log" >/dev/null 2>&1 &
  PID=$!
  for i in $(seq 1 40); do curl -sf "http://127.0.0.1:$port/principles" >/dev/null && break; sleep 0.25; done
  export PRINCIPLES_URL="http://127.0.0.1:$port/principles" FUSEKI_QUERY="http://127.0.0.1:$port/query"
}
teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

@test "the principles come from the door and reach the store's query as a VALUES list" {
  run bash "$PC" --json
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$output" = '{"unexpressed_principles":0,"unenforced_practices":0,"dangling_expresses":0}' ] || { echo "$output"; return 1; }
  grep -q 'VALUES ?p { chorus:hemenway-observe chorus:hemenway-catch-and-store }' "$T/queries.log" || { cat "$T/queries.log"; return 1; }
}

@test "NEGATIVE: no query reads the principles store" {
  run bash "$PC" --json
  ! grep -q 'urn:chorus:domains:principles' "$T/queries.log" || { cat "$T/queries.log"; return 1; }
  ! grep -q 'urn:chorus:domains:principles' "$PC" || return 1
}

@test "NEGATIVE: a door that does not answer is exit 2, never a blind zero" {
  PRINCIPLES_URL=http://127.0.0.1:1/x run bash "$PC" --json
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'refusing to report a blind zero' || { echo "$output"; return 1; }
}
