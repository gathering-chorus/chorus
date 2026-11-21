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
  # the store: two practices, one expressing a real principle, one a principle that is gone
  cat > "$T/store.trig" <<'TRIG'
@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:domains:practices> {
  c:practice-a a c:Practice ; c:expresses c:hemenway-observe ; c:enactedBy c:skill-a .
  c:practice-b a c:Practice ; c:expresses c:xp-gone ; c:enactedBy c:skill-b .
}
TRIG
  printf '%s' '{"count": 2, "data": [{"name": "hemenway-observe"}, {"name": "hemenway-catch-and-store"}]}' > "$T/door.json"
  cat > "$T/stub.py" <<'PY2'
import http.server, os, subprocess, sys, tempfile, urllib.parse
port, log, door, store = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        if u.path == "/principles":
            open(log, "a").write("DOOR " + u.query + "\n")
            b, ct = open(door, "rb").read(), "application/json"
        else:  # the store: the query runs for real (arq over the fixture), so a wrong query answers wrong
            q = urllib.parse.parse_qs(u.query).get("query", [""])[0]
            open(log, "a").write(q.replace("\n", " ") + "\n")
            with tempfile.NamedTemporaryFile("w", suffix=".rq", delete=False) as f:
                f.write(q)
            b = subprocess.run(["arq", "--data", store, "--query", f.name, "--results=CSV"], capture_output=True).stdout
            os.unlink(f.name)
            ct = "text/csv"
        self.send_response(200); self.send_header("Content-Type", ct); self.end_headers(); self.wfile.write(b)
http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
PY2
  python3 "$T/stub.py" "$port" "$T/queries.log" "$T/door.json" "$T/store.trig" >/dev/null 2>&1 &
  PID=$!
  for i in $(seq 1 40); do curl -sf "http://127.0.0.1:$port/principles" >/dev/null && break; sleep 0.25; done
  : > "$T/queries.log"
  export PRINCIPLES_URL="http://127.0.0.1:$port/principles" FUSEKI_QUERY="http://127.0.0.1:$port/query"
}
teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

@test "the principles come from the door and reach the store's query as a VALUES list" {
  run bash "$PC" --json
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }  # a dangling expresses is a failure (exit 1)
  # hemenway-catch-and-store has no practice; practice-b expresses a principle the door does not have
  [ "$output" = '{"unexpressed_principles":1,"unenforced_practices":0,"dangling_expresses":1}' ] || { echo "$output"; return 1; }
  grep -q '^DOOR limit=1000$' "$T/queries.log" || { cat "$T/queries.log"; return 1; }
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

@test "NEGATIVE: a door that holds more principles than it gave is exit 2, never a short list" {
  printf '%s' '{"count": 140, "data": [{"name": "hemenway-observe"}]}' > "$T/door.json"
  run bash "$PC" --json
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "NEGATIVE: a principle name that cannot be read is exit 2, never dropped into dangling" {
  printf '%s' '{"data": [{"name": "hemenway-observe"}, {"name": "Bad Name"}]}' > "$T/door.json"
  run bash "$PC" --json
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "a door that answers with no principles is an empty list: every expresses dangles" {
  printf '%s' '{"count": 0, "data": []}' > "$T/door.json"
  run bash "$PC" --json
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [ "$output" = '{"unexpressed_principles":0,"unenforced_practices":0,"dangling_expresses":2}' ] || { echo "$output"; return 1; }
}
