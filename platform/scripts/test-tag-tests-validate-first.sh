#!/usr/bin/env bash
# test-tag-tests-validate-first.sh — guard for #3970: the tests-domain ingest
# VALIDATES BEFORE IT CLEARS. On 2026-08-21 /domains stopped serving 'deploys'
# and the old clear-first order wiped 1300+ registered Tests down to a partial
# 270 — the ingest destroyed the registry it was refilling. This proves both
# sides (#3734): a bad covers target refuses with ZERO writes issued, and a
# valid corpus still clears + inserts.
#
# HERMETIC: stub owl-api /domains + Fuseki /update on a local port; the tagger
# runs against a fixture repo in a tempdir. No live services touched.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TAGGER="$SCRIPT_DIR/tag-tests-domain.py"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

TMP=$(mktemp -d)
PORT=$(( 20000 + RANDOM % 20000 ))
cleanup() { kill "$SRV_PID" 2>/dev/null; rm -rf "$TMP"; }

# Stub server: GET /domains serves $TMP/domains.json; POST /update appends the
# body to $TMP/updates.log. One server, both doors.
cat > "$TMP/stub.py" <<'PYEOF'
import http.server, sys, urllib.parse
tmp, port = sys.argv[1], int(sys.argv[2])
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        body = open(f"{tmp}/domains.json", "rb").read()
        self.send_response(200); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers()
        self.wfile.write(body)
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        with open(f"{tmp}/updates.log", "a") as f:
            f.write(urllib.parse.unquote_plus(self.rfile.read(n).decode()) + "\n---\n")
        self.send_response(204); self.end_headers()
http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
PYEOF

# Fixture repo: one jest test whose PREFIX rule maps platform/api -> 'services'.
mkdir -p "$TMP/repo/platform/api/tests"
cat > "$TMP/repo/platform/api/tests/sample.test.ts" <<'EOF'
// @test-type: unit
it('adds', () => { expect(1 + 1).toBe(2); });
EOF
git -C "$TMP/repo" init -q 2>/dev/null || true

run_tagger() {
  (cd "$TMP/repo" && OWLAPI="http://127.0.0.1:$PORT" ATHENA_UPDATE="http://127.0.0.1:$PORT/update" \
    python3 "$TAGGER" >"$TMP/out.log" 2>&1)
}

echo '{"data":[{"name":"services"},{"name":"security"}]}' > "$TMP/domains.json"
python3 "$TMP/stub.py" "$TMP" "$PORT" & SRV_PID=$!
for _ in $(seq 1 40); do curl -sf -m 1 "http://127.0.0.1:$PORT/domains" >/dev/null 2>&1 && break; sleep 0.25; done

# 1. Valid corpus: ingest runs, store receives a clear AND inserts.
: > "$TMP/updates.log"
run_tagger; rc=$?
[ "$rc" -eq 0 ] && ok || bad "valid ingest should exit 0, got $rc: $(tail -2 "$TMP/out.log")"
grep -q "DELETE WHERE" "$TMP/updates.log" && ok || bad "valid ingest issues the typed clear"
grep -q "INSERT DATA" "$TMP/updates.log" && ok || bad "valid ingest inserts the corpus"
grep -q 'sample.test.ts' "$TMP/updates.log" && ok || bad "corpus contains the fixture test"

# 2. NEGATIVE PROOF (#3734): the served set loses the covers target ('services'
#    plays tonight's 'deploys'). The ingest must REFUSE with ZERO writes —
#    no clear, no insert; the registry it would have wiped stays intact.
echo '{"data":[{"name":"security"}]}' > "$TMP/domains.json"
: > "$TMP/updates.log"
run_tagger; rc=$?
[ "$rc" -ne 0 ] && ok || bad "unserved covers target must refuse (exit nonzero)"
[ -s "$TMP/updates.log" ] && bad "REFUSAL MUST WRITE NOTHING — updates were issued: $(head -1 "$TMP/updates.log")" || ok
grep -qi "not a generated V2 domain" "$TMP/out.log" && ok || bad "refusal names the missing domain, got: $(tail -2 "$TMP/out.log")"

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
