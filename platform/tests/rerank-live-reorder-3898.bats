#!/usr/bin/env bats
# @test-type: unit — a STATEFUL stub graph (json file) + stub writer that enforces
# the real uniqueness rule. No live Fuseki, no token, no real writes.
#
# #3898 — the proof #3897's fixture did not have.
#
# rerank-3897.bats proved a refusal is SURFACED. It never proved a reorder
# SUCCEEDS, because its stub answered the same CSV regardless of what was
# written and its happy paths ran under DRY_RUN=1 — no write ever met a
# constraint. So the verb shipped, and refused all twelve writes the first time
# it touched Jeff's real chunks.
#
# The difference here: the stub writer REJECTS an ordinal already held by a
# different subject, exactly as chorus:uniqueWithin does, and the reader serves
# whatever the writer last stored. That makes a reorder a real permutation
# against real constraints, which is the only thing that could have caught this.

setup() {
  SCRIPTS="${BATS_TEST_DIRNAME}/../scripts"
  RERANK="$SCRIPTS/chorus-rerank"
  TMP="$(mktemp -d)"
  STATE="$TMP/graph.json"
  PORT=$(( 41000 + RANDOM % 2000 ))

  # The starting world: three chunks holding 1,2,3 — the state that refuses.
  cat > "$STATE" <<'JSON'
{"phone-first": 1, "athena-pipeline": 2, "memory": 3}
JSON

  # Writer: enforces uniqueWithin. Refuses if the ordinal is held by another slug.
  cat > "$TMP/athena-model" <<PY
#!/usr/bin/env python3
import json, sys
STATE = "$STATE"
args = sys.argv[1:]
def field(k):
    for i, a in enumerate(args):
        if a == "--field" and i + 1 < len(args) and args[i+1].startswith(k + "="):
            return args[i+1].split("=", 1)[1]
    return None
slug, seq = field("slug"), field("roleSequence")
if slug is None or seq is None:
    sys.exit(0)                      # not a chunk write; nothing to police
seq = int(seq)
g = json.load(open(STATE))
holder = next((s for s, v in g.items() if v == seq and s != slug), None)
if holder is not None:
    print(f"athena-model: shape-violation: duplicate 'roleSequence' {seq} within 'ownedBy' "
          f"(held by {holder}; chorus:uniqueWithin)", file=sys.stderr)
    sys.exit(1)
g[slug] = seq
json.dump(g, open(STATE, "w"))
PY
  chmod +x "$TMP/athena-model"

  # Reader: serves the writer's state as the projection's CSV.
  cat > "$TMP/serve.py" <<PY
import http.server, socketserver, json
STATE = "$STATE"
class H(http.server.BaseHTTPRequestHandler):
    def _csv(self):
        n = int(self.headers.get('Content-Length') or 0)
        if n: self.rfile.read(n)
        g = json.load(open(STATE))
        rows = "".join(f"{s},{v}\r\n" for s, v in sorted(g.items(), key=lambda kv: kv[1]))
        body = ("slug,rseq\r\n" + rows).encode()
        self.send_response(200); self.send_header('Content-Type','text/csv')
        self.send_header('Content-Length', str(len(body))); self.end_headers()
        self.wfile.write(body)
    do_GET = _csv
    do_POST = _csv
    def log_message(self, *a): pass
socketserver.TCPServer.allow_reuse_address = True
socketserver.TCPServer(("127.0.0.1", $PORT), H).serve_forever()
PY
  python3 "$TMP/serve.py" &
  SERVER_PID=$!
  for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$PORT/" >/dev/null 2>&1 && break; sleep 0.1; done

  export FUSEKI_QUERY_URL="http://127.0.0.1:$PORT/"
  export CHORUS_MODEL_BIN="$TMP/athena-model"
  export CHORUS_IDENTITY_TOKEN="stub-token-not-a-credential"
  export DRY_RUN=0
}

teardown() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
  rm -rf "$TMP"
}

order_now() { python3 -c "
import json;g=json.load(open('$STATE'))
print(' '.join(s for s,_ in sorted(g.items(), key=lambda kv: kv[1])))"; }

@test "NEGATIVE: writing the final ordinals one at a time refuses — the shipped bug" {
  # Exactly what #3897 did: assign 1..N directly. Moving athena-pipeline to 1
  # collides with phone-first, which still holds 1.
  run "$TMP/athena-model" --kind chunk --name athena-pipeline \
      --field slug=athena-pipeline --field roleSequence=1
  [ "$status" -ne 0 ]
  [[ "$output" == *"duplicate 'roleSequence' 1"* ]]
  [[ "$output" == *"held by phone-first"* ]]
  # And the store is untouched — the refusal protected it.
  [ "$(order_now)" = "phone-first athena-pipeline memory" ]
}

@test "the verb reorders live ordinals that already collide" {
  run bash "$RERANK" chunks wren athena-pipeline phone-first
  [ "$status" -eq 0 ]
  [ "$(order_now)" = "athena-pipeline phone-first memory" ]
}

@test "swapping two adjacent ordinals succeeds — the minimal collision" {
  run bash "$RERANK" chunks wren phone-first memory
  [ "$status" -eq 0 ]
  [ "$(order_now)" = "phone-first memory athena-pipeline" ]
}

@test "no parked ordinal survives a successful reorder" {
  run bash "$RERANK" chunks wren memory
  [ "$status" -eq 0 ]
  run python3 -c "
import json; g=json.load(open('$STATE'))
print(max(g.values()))"
  [ "$output" -le 3 ]
}

@test "the reorder is idempotent — running it twice is the same order" {
  bash "$RERANK" chunks wren athena-pipeline memory >/dev/null
  local first; first="$(order_now)"
  run bash "$RERANK" chunks wren athena-pipeline memory
  [ "$status" -eq 0 ]
  [ "$(order_now)" = "$first" ]
}

@test "a mid-reorder failure aborts loudly and names the parked state" {
  # Writer starts refusing everything partway through: the phase-two failure.
  cat > "$TMP/athena-model" <<'SH'
#!/usr/bin/env bash
echo "athena-model: shape-violation: duplicate 'roleSequence' within 'ownedBy'" >&2
exit 1
SH
  chmod +x "$TMP/athena-model"
  run bash "$RERANK" chunks wren memory
  [ "$status" -ne 0 ]
  [[ "$output" == *"ABORTED mid-reorder"* ]]
  [[ "$output" == *"PARKED"* ]]
}
