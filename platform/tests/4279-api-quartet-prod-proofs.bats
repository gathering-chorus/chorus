#!/usr/bin/env bats
# @test-type: integration — NEGATIVE PROOFS (#3734) for 4279-api-quartet-prod.bats.
# @domain: tests — the product domain this suite guards (#4334)
# Neither case writes production: one proves the label gate refuses, the other
# plants a probe row in a throwaway graph and proves the residue query finds it.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
RUNNER="$ROOT/platform/tests/4267-all-generated-apis.test.sh"
API="${QUARTET_API:-http://localhost:3360}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"

setup() {
  # shellcheck disable=SC1091
  [ -r "$ROOT/platform/scripts/fuseki-auth.sh" ] && source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null
  curl -sf --max-time 5 "$API/health" >/dev/null 2>&1 || skip "$API not answering"
}

@test "an unlabelled production run is refused (rc=3) and names the label" {
  run env API_BASE="$API" CHORUS_CONTEXT=test QUARTET_PROD= bash "$RUNNER"
  echo "$output" | head -3
  [ "$status" -eq 3 ]
  echo "$output" | grep -q "not labelled a production write"
}

@test "the residue query finds a planted leftover row and not an absent one" {
  G="urn:chorus:test:4279-fixture-$$"; RID="proof-$$"
  code="$(curl -s --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "INSERT DATA { GRAPH <$G> { <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-planted> <https://jeffbridwell.com/chorus#label> \"planted\" } }" "$UPDATE")"
  case "$code" in 2*) ;; *) echo "could not plant the fixture row (HTTP $code)"; return 1 ;; esac
  left="$(curl -s --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -G "$QUERY" \
    --data-urlencode "query=SELECT DISTINCT ?g ?s WHERE { VALUES ?s { <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-planted> <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-absent> } GRAPH ?g { ?s ?p ?o } }" \
    -H 'Accept: text/csv' 2>/dev/null | tail -n +2 | tr -d '\r')"
  curl -s --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -X POST -H 'Content-Type: application/sparql-update' --data-binary "DROP SILENT GRAPH <$G>" "$UPDATE"
  echo "found: $left"
  echo "$left" | grep -q "zz-probe-$RID-planted"
  ! echo "$left" | grep -q "zz-probe-$RID-absent"
}

# #4282 — the runner must send a constrained field one of the manifest's own
# allowedValues. Before today the fields were tab-joined, and `read` collapses
# consecutive tabs: a literal with no targetClass had its allowed value read as
# the target and its datatype read as the value, so the door got the word
# "string" where the shape says pc|xp. Thirteen classes read "runner input
# rejected" for that alone. This proof stands up a recording API and a fixture
# manifest, then reads the create body the runner actually sent.
@test "the runner sends a constrained field its first allowed value (never the datatype word) and a lowercase subject" {
  FIX="$BATS_TEST_TMPDIR"
  cat >"$FIX/gen" <<'GEN'
#!/usr/bin/env bash
cat <<'JSON'
{"quartet":{"throwawaySubject":"zz-fixture-widget","steps":[
 {"step":"create","method":"POST","path":"/widgets/widgets","expectStatus":201,"readBack":{"compareFields":[]}},
 {"step":"read","method":"GET","path":"/widgets/widgets/zz-fixture-widget","expectStatus":200},
 {"step":"update","method":"PUT","path":"/widgets/widgets/zz-fixture-widget","expectStatus":200,"readBack":{"compareFields":[]}},
 {"step":"delete","method":"DELETE","path":"/widgets/widgets/zz-fixture-widget","expectStatus":200}]},
 "requiredFields":[
  {"field":"label","kind":"literal"},
  {"field":"widgetKind","kind":"literal","datatype":"string","allowedValues":["pc","xp"]},
  {"field":"ownedBy","kind":"literal"}]}
JSON
GEN
  printf '#!/usr/bin/env bash\necho fixture-token\n' >"$FIX/token"
  chmod +x "$FIX/gen" "$FIX/token"
  cat >"$FIX/api.py" <<'PY'
import json,sys
from http.server import BaseHTTPRequestHandler,HTTPServer
rec=sys.argv[2]
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def _send(self,code,body):
        b=json.dumps(body).encode(); self.send_response(code)
        self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(b)))
        self.end_headers(); self.wfile.write(b)
    def do_GET(self):
        if self.path=="/": return self._send(200,{"primitives":[{"kind":"Widget"}]})
        if self.path=="/health": return self._send(200,{"ok":True})
        return self._send(200,{"data":{"name":"zz-fixture-widget"}})
    def do_POST(self):
        n=int(self.headers.get("Content-Length","0")); open(rec,"wb").write(self.rfile.read(n))
        return self._send(201,{"status":"created"})
    def do_PUT(self):
        n=int(self.headers.get("Content-Length","0")); self.rfile.read(n); return self._send(200,{"status":"ok"})
    def do_DELETE(self): return self._send(200,{"status":"deleted"})
HTTPServer(("127.0.0.1",int(sys.argv[1])),H).serve_forever()
PY
  PORT=$((20000 + RANDOM % 20000))
  python3 "$FIX/api.py" "$PORT" "$FIX/create.json" & SRV=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break; sleep 0.3; done
  # The run id carries capitals on purpose: the DAL slugs the name it writes
  # (T→t), the door reads by the literal path, so a capital in the subject is a
  # 404 read-back and a leftover row (16:53 run, 2026-09-23: 138 rows). The
  # runner must send the slug.
  run env API_BASE="http://127.0.0.1:$PORT" CHORUS_ATHENA_MAKE="$FIX/gen" CHORUS_TOKEN_BIN="$FIX/token" \
      CHORUS_CONTEXT=test QUARTET_PROD= QUARTET_RUN_ID="Proof-$$-CAPS" bash "$RUNNER"
  kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null || true
  echo "$output" | grep -E '^Widget ' || true
  [ -s "$FIX/create.json" ] || { echo "the runner never sent a create body"; return 1; }
  echo "sent: $(cat "$FIX/create.json")"
  sent="$(python3 -c 'import json,sys; b=json.load(open(sys.argv[1])); print(b.get("widgetKind",""), b.get("name",""), b.get("label",""))' "$FIX/create.json")"
  read -r kind name label <<<"$sent"
  [ "$kind" = "pc" ]
  [ "$name" = "zz-probe-proof-$$-caps-widget" ]
  case "$label" in zz-4267-*) ;; *) echo "label was $label"; return 1 ;; esac
  ! grep -q '"string"' "$FIX/create.json"
}
