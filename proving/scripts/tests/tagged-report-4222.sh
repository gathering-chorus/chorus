#!/usr/bin/env bash
# @test-type: unit
#
# #4222 — NEGATIVE PROOFS for platform/scripts/tagged-report.
#
# The report gates the card's goal, so per #3734 it ships with fixtures where
# the guarded condition is VIOLATED and the check is shown to FAIL. Three of
# these caught real hollow checks at the 2026-09-20 gate: WEED was hardcoded 0
# while the docstring advertised it, an absent servedFrom made MOVE silently
# zero, and an unset CHORUS_ROOT fell back to one machine's home directory.
#
# Brings its own world: a stub door on a free port, a temp tree. No live stack.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
REPORT="$ROOT/platform/scripts/tagged-report"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; [ -n "${SRV:-}" ] && kill "$SRV" 2>/dev/null' EXIT
pass=0; fail=0
ok()   { echo "  ok   $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL $1"; fail=$((fail+1)); }

# --- the stub door -----------------------------------------------------------
# FIXTURE knobs, one per guarded condition. Each run serves exactly the state
# the check exists to catch.
cat > "$TMP/door.py" <<'PY'
import json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
MODE = os.environ["FIXTURE"]
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        route = self.path.split("?")[0].lstrip("/")
        body = {"servedFrom": "urn:chorus:domains:code", "data": []}
        if route == "code/files":
            if MODE == "wrong-graph":
                body["servedFrom"] = "urn:chorus:instances"
                body["data"] = [{"filePath": "README.md", "hasDomain": "code"}]
            elif MODE == "no-servedfrom":
                body.pop("servedFrom")
                body["data"] = [{"filePath": "README.md", "hasDomain": "code"}]
            elif MODE == "untagged":
                body["data"] = [{"filePath": "README.md"}]
            elif MODE == "ghost":
                body["data"] = [{"filePath": "gone-from-the-tree.md", "hasDomain": "code"}]
            else:
                body["data"] = [{"filePath": "README.md", "hasDomain": "code"}]
        elif route == "tests/tests":
            body["servedFrom"] = "urn:chorus:domains:tests"
            body["data"] = [{"filePath": "a.test.ts", "covers": "tests"}]
        elif route == "logs/sources":
            body["servedFrom"] = "urn:chorus:domains:logs"
            body["data"] = [{"name": "x", "hasDomain": "logs"}]
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())
srv = HTTPServer(("127.0.0.1", 0), H)
print(srv.server_port, flush=True)
srv.serve_forever()
PY

run_fixture() { # <fixture> <root> -> writes $TMP/out, returns the exit code
  FIXTURE="$1" python3 "$TMP/door.py" > "$TMP/port" 2>/dev/null &
  SRV=$!
  for _ in $(seq 50); do [ -s "$TMP/port" ] && break; sleep 0.1; done
  local port; port="$(cat "$TMP/port")"
  CHORUS_OWL_API="http://127.0.0.1:$port" CHORUS_ROOT="$2" \
    HOME="$TMP" python3 "$REPORT" > "$TMP/out" 2>&1
  local rc=$?
  kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null; SRV=""
  : > "$TMP/port"
  return $rc
}

# Every path the stub door serves must EXIST here, or WEED fires on the fixture
# rather than on the state under test — the #3725 bogus-fixture shape.
mkdir -p "$TMP/tree" && : > "$TMP/tree/README.md" && : > "$TMP/tree/a.test.ts"

# 1. the clean state passes — without this the others prove nothing
run_fixture clean "$TMP/tree"; rc=$?
[ $rc -eq 0 ] && ok "a clean door exits 0" || { bad "clean door should exit 0, got $rc"; sed 's/^/    /' "$TMP/out"; }
grep -q "clean" "$TMP/out" || bad "clean door should say clean"

# 2. NEGATIVE PROOF — a row in the wrong graph is MOVE, not silence
run_fixture wrong-graph "$TMP/tree"; rc=$?
[ $rc -ne 0 ] && ok "wrong graph is red" || bad "wrong graph passed — MOVE never fires"
grep -q "MOVE" "$TMP/out" || bad "wrong graph must name MOVE"

# 3. NEGATIVE PROOF — a door that does not say where it read is UNMEASURED
run_fixture no-servedfrom "$TMP/tree"; rc=$?
[ $rc -ne 0 ] && ok "absent servedFrom is red" || bad "absent servedFrom passed"
grep -q "UNMEASURED" "$TMP/out" || bad "absent servedFrom must read UNMEASURED"

# 4. NEGATIVE PROOF — an untagged row is FEED and is NAMED
run_fixture untagged "$TMP/tree"; rc=$?
[ $rc -ne 0 ] && ok "untagged row is red" || bad "untagged row passed"
grep -q "FEED" "$TMP/out" && grep -q "README.md" "$TMP/out" \
  && ok "FEED names the row" || bad "FEED must name the row, not just count it"

# 5. NEGATIVE PROOF — a row with no file behind it is WEED
run_fixture ghost "$TMP/tree"; rc=$?
[ $rc -ne 0 ] && ok "ghost row is red" || bad "ghost row passed — WEED never fires"
grep -q "WEED" "$TMP/out" || bad "ghost row must name WEED"

# 6. NEGATIVE PROOF — no CHORUS_ROOT is UNMEASURED, never a clean read
run_fixture clean "$TMP/does-not-exist"; rc=$?
[ $rc -ne 0 ] && ok "missing CHORUS_ROOT is red" || bad "missing root passed as clean"
grep -q "CHORUS_ROOT" "$TMP/out" || bad "missing root must say so"

echo "=== Results: $pass passed, $fail failed ==="
[ $fail -eq 0 ]
