#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: a stub SPARQL endpoint answers the
# @domain: knowledge — the product domain this suite guards (#4334)
# copy verification; no live store, no writes.
# #4187 — graph-rehome verifies a copy landed before anything is pruned. The
# first version compared raw row counts between the two graphs, which refused a
# correct Document copy on 2026-09-18 (12 in the source, 19 already in the
# destination) and would equally have PASSED a short copy into an empty
# destination that happened to match. The check now asks the one question that
# separates those states: is any source triple still missing downstream?

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/graph-rehome-4187.sh"

stub_start() {  # $1 = src rows · $2 = dst rows · $3 = source triples missing from dst
  TMP="$(mktemp -d)"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')
  cat > "$TMP/stub.py" <<'PY'
import json, sys, urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer
SRC, DST, MISSING = (int(x) for x in sys.argv[2:5])
def answer(q):
    if "FILTER NOT EXISTS" in q:
        return MISSING
    if "<urn:chorus:instances>" in q:
        return SRC
    return DST
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0)); body = self.rfile.read(n).decode()
        q = urllib.parse.parse_qs(body).get("query", [""])[0]
        if not q:  # an update POST
            self.send_response(204); self.end_headers(); return
        out = json.dumps({"head": {"vars": ["n"]},
                          "results": {"bindings": [{"n": {"value": str(answer(q))}}]}}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/sparql-results+json")
        self.send_header("Content-Length", str(len(out))); self.end_headers(); self.wfile.write(out)
    def do_GET(self): self.do_POST()
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
  python3 "$TMP/stub.py" "$PORT" "$1" "$2" "$3" & STUB_PID=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -sf --max-time 1 --data-urlencode 'query=SELECT 1' "http://127.0.0.1:$PORT/query" >/dev/null 2>&1 && break
    sleep 0.2
  done
  export FUSEKI_QUERY="http://127.0.0.1:$PORT/query" FUSEKI_UPDATE="http://127.0.0.1:$PORT/update"
}
teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null; rm -rf "${TMP:-}"; }

@test "NEGATIVE PROOF — a copy that left triples behind is refused and says how many" {
  stub_start 12 12 4
  run bash "$SCRIPT" Document documents --go
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "REFUSED: copy is short — 4 triple(s) still only in urn:chorus:instances"
}

@test "a complete copy into a destination that already holds MORE rows is accepted" {
  stub_start 12 19 0
  run bash "$SCRIPT" Document documents --go
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "copy verified"
}

@test "the old count comparison would have called this complete copy short" {
  stub_start 12 19 0
  run bash "$SCRIPT" Document documents --go
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "after copy: urn:chorus:instances=12  urn:chorus:domains:documents=19"
}
