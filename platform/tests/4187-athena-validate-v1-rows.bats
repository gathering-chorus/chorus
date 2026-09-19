#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: a stub SPARQL endpoint on a local port answers the sweep; no live store, no writes
# #4187 — athena-validate counts the rows still living in the two v1 graphs
# (urn:chorus:instances, urn:chorus:ontology) per class, and a single leftover row
# turns the sweep red. Jeff 2026-09-16: "no more chorus:ontology or chorus:instances
# urns", "goal is to get and stay at 0". The check must separate the two states.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# #4167 — the bash retired; this suite drives the Rust verb.
SCRIPT="$REPO_ROOT/platform/services/athena-validate/target/release/athena-validate"

stub_start() {  # $1 = v1 rows for class File · $2 = ownedBy objects that are not a Principal
  TMP="$(mktemp -d)"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')
  cat > "$TMP/stub.py" <<PY
import json, sys, urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer
ROWS = int(sys.argv[2])
OWNERS = int(sys.argv[3]) if len(sys.argv) > 3 else 0
# #4167 — the verb asks for text/csv, which is what Fuseki returns when asked.
# The stub used to answer JSON whatever the Accept header said, so a CSV reader
# parsed a JSON blob into nonsense rows and every assertion here failed for a
# reason that had nothing to do with the check under test. A stub that ignores
# Accept is not modelling the store.
#
# One row per violation, not a grouped count: the same change made to the checks
# themselves. A count nobody can trace to a row is how the bash reported numbers
# no one could act on.
def csv_rows(q):
    if q.strip().startswith("ASK"):
        return "boolean\ntrue\n"
    if "VALUES ?g { <urn:chorus:instances> <urn:chorus:ontology> }" in q:
        out = ["s,c"]
        for i in range(ROWS):
            out.append("https://jeffbridwell.com/chorus#file-%d,https://jeffbridwell.com/chorus#File" % i)
        return "\n".join(out) + "\n"
    if "c:ownedBy ?o" in q:
        out = ["s,o"]
        for i in range(OWNERS):
            out.append("https://jeffbridwell.com/chorus#row-%d,crawler" % i)
        return "\n".join(out) + "\n"
    return "s\n"
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0)); body = self.rfile.read(n).decode()
        q = urllib.parse.parse_qs(body).get("query", [""])[0]
        out = csv_rows(q).encode()
        self.send_response(200); self.send_header("Content-Type", "text/csv"); self.send_header("Content-Length", str(len(out))); self.end_headers(); self.wfile.write(out)
    def do_GET(self): self.do_POST()
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
  python3 "$TMP/stub.py" "$PORT" "$1" "${2:-0}" & STUB_PID=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -sf --max-time 1 --data-urlencode 'query=ASK { }' "http://127.0.0.1:$PORT/query" >/dev/null 2>&1 && break; sleep 0.2; done
}
teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null; rm -rf "${TMP:-}"; }

@test "NEGATIVE PROOF — one class with rows left in urn:chorus:instances turns the sweep red, per class, with the count" {
  stub_start 3
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:$PORT/query" "$SCRIPT" --store-only
  [ "$status" -eq 1 ]
  # Per-row now, not a grouped count (#4167, Jeff ok 17:53).
  [ "$(grep -c "^graph-issue|v1-row|" "$TMP/gv.txt")" = "3" ]
  grep -q "^graph-issue|v1-row|file-0|" "$TMP/gv.txt"
  grep -q "^graph-summary|3|dirty$" "$TMP/gv.txt"
}

@test "zero rows in both v1 graphs is clean, and says so on its own line" {
  stub_start 0
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:$PORT/query" "$SCRIPT" --store-only
  [ "$status" -eq 0 ]
  # #4167 — the bash printed a prose reassurance line; the Rust says it by
  # ABSENCE plus the summary, which is the stronger statement: no v1-row issue
  # exists at all, and the verdict is clean.
  test -z "$(grep -F "graph-issue|v1-row|" "$TMP/gv.txt" || true)"
  ! grep -q "^graph-issue|v1-row|" "$TMP/gv.txt"
  grep -q "^graph-summary|0|clean$" "$TMP/gv.txt"
}

@test "NEGATIVE PROOF — an ownedBy whose object is not a Principal turns the sweep red, named and counted" {
  stub_start 0 5
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:$PORT/query" "$SCRIPT" --store-only
  [ "$status" -eq 1 ]
  [ "$(grep -c "^graph-issue|owner-not-principal|" "$TMP/gv.txt")" = "5" ]
  grep -q "^graph-issue|owner-not-principal|row-0|crawler$" "$TMP/gv.txt"
  # The summary TOTAL is deliberately not pinned to 5. It is the sum across every
  # check in the sweep, so pinning it makes this test fail whenever an unrelated
  # check is added - which is what happened on 2026-09-19. The two things this
  # negative proof is actually about are that the violation is NAMED with its
  # count (asserted above) and that the verdict flips to dirty, so those are what
  # is asserted. A number that changes for reasons outside the behaviour under
  # test is a brittle assert, not a stronger one.
  grep -q "^graph-summary|[0-9]*|dirty$" "$TMP/gv.txt"
  test -z "$(grep "^graph-summary|0|clean$" "$TMP/gv.txt" || true)"
}

@test "every ownedBy object being a Principal is clean, and the owner line says so" {
  stub_start 0 0
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:$PORT/query" "$SCRIPT" --store-only
  [ "$status" -eq 0 ]
  # Clean is said by ABSENCE plus the verdict, not by a prose line (#4167).
  test -z "$(grep -F "graph-issue|owner-not-principal|" "$TMP/gv.txt" || true)"
  grep -q "^graph-summary|0|clean$" "$TMP/gv.txt"
  ! grep -q "^graph-issue|owner-not-principal|" "$TMP/gv.txt"
}

@test "the baseline no longer lives on the subject the deploy stamp wipes" {
  YML="$REPO_ROOT/.github/workflows/athena.yml"
  ! grep -q "<urn:chorus:model-deploy> <urn:chorus:vocab#validateIssues>" "$YML"
  grep -q "<urn:chorus:model-validate> <urn:chorus:vocab#validateIssues>" "$YML"
  # the stamp rewrite in the deployer still deletes every predicate of model-deploy:
  # that is exactly why the baseline had to move (if this changes, the reason is gone, not the rule)
  grep -q 'DELETE WHERE { GRAPH <$ONTOLOGY_GRAPH> { <urn:chorus:model-deploy> ?p ?o } }' "$REPO_ROOT/platform/scripts/athena-deploy-model.sh"
}
