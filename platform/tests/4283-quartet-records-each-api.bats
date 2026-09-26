#!/usr/bin/env bats
# @test-type: unit — a stub API captures the posts; nothing live is touched
# @domain: tests — the product domain this suite guards (#4334)
# #4283 — one TestResult row per generated API per owner. NEGATIVE PROOF: a
# walk with one failing class produces exactly one failing row naming it.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  python3 - "$T" <<'PY' &
import sys, json, http.server, socketserver
T = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(s):
        n = int(s.headers.get("Content-Length") or 0)
        body = s.rfile.read(n).decode() if n else ""
        open(T + "/posts.jsonl", "a").write(s.path + " " + body + "\n")
        s.send_response(201); s.end_headers()
    def log_message(s, *a): pass
srv = socketserver.TCPServer(("127.0.0.1", 0), H)
open(T + "/port", "w").write(str(srv.server_address[1]))
srv.serve_forever()
PY
  STUB_PID=$!
  for _ in $(seq 1 50); do [ -s "$T/port" ] && break; sleep 0.1; done
  API="http://127.0.0.1:$(cat "$T/port")"
  source "$ROOT/platform/tests/lib/quartet-record.sh"
}
teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null || true; }

posted() { grep -c '^/v1/tests/results ' "$T/posts.jsonl" 2>/dev/null || echo 0; }
rows_where() { python3 -c '
import sys,json
key,val=sys.argv[2:4]; n=0
for l in open(sys.argv[1]):
    d=json.loads(l.split(" ",1)[1])
    if d.get(key)==val: n+=1
print(n)' "$T/posts.jsonl" "$1" "$2"; }

@test "#4283 one walk of three classes with one failure posts three rows, exactly one of them failing and it names the class" {
  quartet_record "$API" tok kade Nudge PASS "create read update delete, fields survived, no residue"
  quartet_record "$API" tok kade Page  FAIL "create POST wanted 201 got 409"
  quartet_record "$API" tok kade Role  PASS "ok"
  [ "$(posted)" -eq 3 ]
  [ "$(rows_where result fail)" -eq 1 ]
  [ "$(rows_where testName 'kade: Page quartet')" -eq 1 ]
  [ "$(rows_where filePath platform/tests/4279-api-quartet-prod.bats)" -eq 3 ]
}

@test "#4283 NOT-PERM and UNMEASURED verdicts are stored as unmeasured, never as pass or fail" {
  quartet_record "$API" tok wren Principal NOT-PERM "deploy-only"
  quartet_record "$API" tok wren LogSource UNMEASURED "no row to point a required edge at"
  [ "$(posted)" -eq 2 ]
  [ "$(rows_where result unmeasured)" -eq 2 ]
  [ "$(rows_where result pass)" -eq 0 ]
  [ "$(rows_where result fail)" -eq 0 ]
}

@test "#4283 the row carries the owner's registered case as ofTest when one is given, and no ofTest when none is" {
  quartet_record "$API" tok silas Metric PASS ok "https://jeffbridwell.com/chorus#test-x"
  quartet_record "$API" tok silas Nudge  PASS ok
  # the API takes the edge target as a local name: a full IRI came back 422 on 2026-09-24 02:30
  [ "$(rows_where ofTest 'test-x')" -eq 1 ]
  [ "$(rows_where ofTest 'https://jeffbridwell.com/chorus#test-x')" -eq 0 ]
  [ "$(posted)" -eq 2 ]
}

@test "#4283 QUARTET_RECORD=0 records nothing (the seam the werk-only runs use)" {
  QUARTET_RECORD=0 quartet_record "$API" tok kade Nudge PASS ok
  [ "$(posted)" -eq 0 ]
}
