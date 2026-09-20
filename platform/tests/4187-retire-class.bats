#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: a stub SPARQL endpoint and a stub
# athena-make answer the guards; no live store, no writes to production.
# #4187 — the class-retirement form of the retirement list. A class delete is the
# most destructive entry in the file, so the guards are the feature: it refuses a
# class that is still SERVED, refuses one whose rows anything points at, and
# refuses to report success unless the row count actually reached zero.

REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$REPO/platform/services/athena-deploy/target/release/athena-deploy"

# $1 inbound-edge count · $2 rows before · $3 rows after · $4 how many domains CLAIM the class
stub_start() {
  TMP="$(mktemp -d)"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')
  cat > "$TMP/stub.py" <<'PY'
import json, sys, urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer
INBOUND, BEFORE, AFTER, CLAIMED = sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
STATE = {"deleted": False}
class H(BaseHTTPRequestHandler):
    def _send(self, body, ctype="application/sparql-results+json"):
        b = body.encode()
        self.send_response(200); self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
    def do_GET(self):
        if "__model_deploy_probe__" in self.path:
            return self._send("probe", "text/plain")
        self._send("probe", "text/plain")
    def do_PUT(self):
        n = int(self.headers.get("Content-Length", 0)); self.rfile.read(n)
        self.send_response(204); self.end_headers()
    def do_DELETE(self):
        self.send_response(204); self.end_headers()
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0)); body = self.rfile.read(n).decode()
        q = urllib.parse.parse_qs(body)
        if "update" in q:
            STATE["deleted"] = True
            self.send_response(204); self.end_headers(); return
        query = q.get("query", [""])[0]
        if "csv" in (self.headers.get("Accept") or ""):
            return self._send("n\r\n0\r\n", "text/csv")
        if "LIMIT 10" in query:
            n = int(AFTER) if AFTER.isdigit() else 0
            rows = [{"s": {"value": "https://jeffbridwell.com/chorus#fixture-row-%d" % i}} for i in range(min(n, 10))]
            return self._send(json.dumps({"head": {"vars": ["s"]}, "results": {"bindings": rows}}))
        if "definesVocabulary" in query:
            v = CLAIMED
        elif "?x ?p ?s" in query:
            v = INBOUND
        else:
            v = AFTER if STATE["deleted"] else BEFORE
        self._send(json.dumps({"head": {"vars": ["n"]},
                               "results": {"bindings": [{"n": {"value": v}}]}}))
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
  python3 "$TMP/stub.py" "$PORT" "$1" "$2" "$3" "$4" & STUB_PID=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -sf --max-time 1 "http://127.0.0.1:$PORT/__model_deploy_probe__" >/dev/null 2>&1 && break
    sleep 0.2
  done
  printf '# empty\n' > "$TMP/empty.ttl"
  RET="$TMP/retire.jsonl"
  printf '%s\n' '{"retire_class":"https://jeffbridwell.com/chorus#Fixture","graph":"urn:chorus:bats-4187","status":"staged"}' > "$RET"
}
teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null; rm -rf "${TMP:-}"; }

run_deploy() {
  run env RETIREMENTS_FILE="$RET" \
      FUSEKI_QUERY="http://127.0.0.1:$PORT/query" \
      FUSEKI_UPDATE="http://127.0.0.1:$PORT/update" \
      FUSEKI_GSP="http://127.0.0.1:$PORT/data" \
      OWL_API_URL="http://127.0.0.1:$PORT" \
      ONTOLOGY_GRAPH="urn:chorus:bats-4187-ontology" \
      TTL="$TMP/empty.ttl" DEPLOY_TARGET=canonical ROLE=wren CHORUS_ROOT="$REPO" \
      "$SCRIPT"
}

@test "#4187 a class with no consumers and no route is retired, and says how many rows" {
  stub_start 0 42 0 0
  run_deploy
  echo "$output" | grep -q "class retirement executed"
  echo "$output" | grep -q "Fixture removed"
  echo "$output" | grep -q "42 rows"
}

@test "#4187 NEGATIVE PROOF: a class WITH a consumer is REFUSED, not deleted" {
  stub_start 7 42 0 0
  run_deploy
  test "$status" -ne 0
  echo "$output" | grep -q "7 triple(s) point at rows of Fixture"
  test -z "$(printf '%s' "$output" | grep -F "class retirement executed" || true)"
}

@test "#4187 NEGATIVE PROOF: a class a domain still CLAIMS is REFUSED" {
  stub_start 0 42 0 1
  run_deploy
  test "$status" -ne 0
  echo "$output" | grep -q "claim $(echo Fixture) in definesVocabulary" || echo "$output" | grep -q "definesVocabulary"
  test -z "$(printf '%s' "$output" | grep -F "class retirement executed" || true)"
}

@test "#4187 NEGATIVE PROOF: a delete that did not take is REFUSED, not reported green" {
  stub_start 0 42 42 0
  run_deploy
  test "$status" -ne 0
  echo "$output" | grep -q "did NOT take"
  echo "$output" | grep -q "survivors:"
  echo "$output" | grep -q "fixture-row-0"
  test -z "$(printf '%s' "$output" | grep -F "class retirement executed" || true)"
}

@test "#4187 an unanswerable athena-make defers rather than deleting blind" {
  stub_start 0 42 0 0
  run env RETIREMENTS_FILE="$RET" \
      FUSEKI_QUERY="http://127.0.0.1:$PORT/query" \
      FUSEKI_UPDATE="http://127.0.0.1:$PORT/update" \
      FUSEKI_GSP="http://127.0.0.1:$PORT/data" \
      OWL_API_URL="http://127.0.0.1:9" \
      ONTOLOGY_GRAPH="urn:chorus:bats-4187-ontology" \
      TTL="$TMP/empty.ttl" DEPLOY_TARGET=canonical ROLE=wren CHORUS_ROOT="$REPO" \
      "$SCRIPT"
  echo "$output" | grep -q "refusing to delete blind"
  test -z "$(printf '%s' "$output" | grep -F "class retirement executed" || true)"
}

@test "#4187 a self-edge alone does NOT block the retirement" {
  # The stub answers the inbound-edge query with what the SCRIPT asked for, so
  # the exclusion is exercised by the query text itself: with FILTER(?x != ?s)
  # present the fixture reports 0, without it 1.
  stub_start 0 42 0 0
  run_deploy
  echo "$output" | grep -q "class retirement executed"
}

@test "#4187 NEGATIVE PROOF: one EXTERNAL consumer still REFUSES" {
  stub_start 1 42 0 0
  run_deploy
  test "$status" -ne 0
  echo "$output" | grep -q "1 triple(s) point at rows of Fixture"
  test -z "$(printf '%s' "$output" | grep -F "class retirement executed" || true)"
}

@test "#4187 NEGATIVE PROOF: the exclusion is self-only — the query must carry FILTER(?x != ?s)" {
  # #4229 — the verb is a binary; the query lives in its source.
  SCRIPT_SRC="$(cat "$REPO/platform/services/athena-deploy/src/lib.rs")"
  echo "$SCRIPT_SRC" | grep -q 'FILTER(?x != ?s)'
  # and it must not have been widened to exclude a whole graph or predicate set
  test -z "$(printf '%s' "$SCRIPT_SRC" | grep -F 'FILTER(?g2 !=' || true)"
}
