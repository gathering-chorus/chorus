#!/usr/bin/env bats
# @test-type: unit — stub SPARQL endpoint on a scratch port + stub writer; no live
# Fuseki, no minted token, no real writes.
# #3897 — chorus-rerank: declared chunk/card order, and the refusals that keep it honest.
#
# Brings its own world (#3528): a stub SPARQL endpoint on a scratch port and a
# stub athena-model. Touches no live Fuseki, mints no token, writes nothing real.
#
# NEGATIVE PROOFS (#3734), because every happy path here would pass on a script
# that quietly did the wrong thing:
#   • "surfaces a DAL refusal" — the DAL refuses duplicate ordinals (#3681). A
#     re-rank that printed success while the graph refused would tell Jeff his
#     order was applied when it was not. The fixture makes athena-model FAIL and
#     requires non-zero exit + the conflict text.
#   • "refuses an unknown slug BEFORE writing" — a typo must not create a ghost
#     chunk at position 1, burying the real work under something empty.
#   • "an unnamed chunk is not silently dropped" — the partial-order case. A
#     script that renumbered only what was named would leave the rest at stale
#     ordinals, colliding or vanishing from the walk.

setup() {
  SCRIPTS="${BATS_TEST_DIRNAME}/../scripts"
  RERANK="$SCRIPTS/chorus-rerank"
  TMP="$(mktemp -d)"
  PORT=$(( 39000 + RANDOM % 2000 ))

  # Stub SPARQL: any query gets the same CSV. Enough — the script only ever asks
  # two questions and parses slug/ordinal columns.
  cat > "$TMP/serve.py" <<PY
import http.server, socketserver
CSV = b"slug,rseq\r\nphone-first,1\r\nathena-pipeline,2\r\nmemory,3\r\n"
class H(http.server.BaseHTTPRequestHandler):
    def _csv(self):
        n = int(self.headers.get('Content-Length') or 0)
        if n: self.rfile.read(n)          # drain the query body
        self.send_response(200); self.send_header('Content-Type','text/csv')
        self.send_header('Content-Length', str(len(CSV))); self.end_headers()
        self.wfile.write(CSV)
    # The DAL reads go out as POST (curl --data-urlencode). A GET-only stub made
    # every read empty and the script FATAL'd on 'owns no chunks' — a fixture
    # failing for the wrong reason, which is worse than no fixture.
    do_GET = _csv
    do_POST = _csv
    def log_message(self, *a): pass
socketserver.TCPServer.allow_reuse_address = True
socketserver.TCPServer(("127.0.0.1", $PORT), H).serve_forever()
PY
  python3 "$TMP/serve.py" &
  SERVER_PID=$!
  for _ in $(seq 1 40); do
    curl -sf "http://127.0.0.1:$PORT/" >/dev/null 2>&1 && break
    sleep 0.1
  done

  export FUSEKI_QUERY_URL="http://127.0.0.1:$PORT/"
  export DRY_RUN=1   # happy paths do not need the writer or a token
}

teardown() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
  rm -rf "$TMP"
}

@test "a stated chunk order is applied first-to-last" {
  run bash "$RERANK" chunks wren athena-pipeline phone-first
  [ "$status" -eq 0 ]
  [[ "$output" == *"chunk athena-pipeline → roleSequence 1"* ]]
  [[ "$output" == *"chunk phone-first → roleSequence 2"* ]]
}

@test "an unnamed chunk is not silently dropped — it follows the named ones" {
  run bash "$RERANK" chunks wren athena-pipeline
  [ "$status" -eq 0 ]
  [[ "$output" == *"chunk athena-pipeline → roleSequence 1"* ]]
  # phone-first and memory were not named; both must still be ordered, after it,
  # keeping their relative order (1 then 3 → 2 then 3).
  [[ "$output" == *"chunk phone-first → roleSequence 2"* ]]
  [[ "$output" == *"chunk memory → roleSequence 3"* ]]
}

@test "NEGATIVE: an unknown slug refuses BEFORE any write" {
  run bash "$RERANK" chunks wren nope-not-a-chunk
  [ "$status" -ne 0 ]
  [[ "$output" == *"not a chunk owned by wren"* ]]
  # The proof that matters: nothing was ordered, not even the valid chunks.
  [[ "$output" != *"roleSequence 1"* ]]
}

@test "NEGATIVE: a DAL refusal is surfaced and exits non-zero" {
  # The state this test exists to catch: the graph refuses (duplicate ordinal,
  # #3681) and the script reports success anyway. Stub writer always fails.
  cat > "$TMP/athena-model" <<'SH'
#!/usr/bin/env bash
echo "refused: chorus:roleSequence 1 already held by chunk-phone-first (uniqueness #3681)" >&2
exit 1
SH
  chmod +x "$TMP/athena-model"

  DRY_RUN=0 \
  CHORUS_MODEL_BIN="$TMP/athena-model" \
  CHORUS_IDENTITY_TOKEN="stub-token-not-a-credential" \
  run bash "$RERANK" chunks wren athena-pipeline

  [ "$status" -ne 0 ]
  [[ "$output" == *"REFUSED"* ]]
  # The conflicting pair must reach the caller — it is the only thing that says
  # WHICH ordinal collided, and a bare "refused" sends someone reading SPARQL.
  [[ "$output" == *"uniqueness #3681"* ]]
}

@test "a stated card order within a chunk is applied first-to-last" {
  run bash "$RERANK" cards phone-first athena-pipeline
  # The stub serves the same CSV for members, so column one is the id set; the
  # order verb must accept a listed member and rank it 1.
  [ "$status" -eq 0 ]
  [[ "$output" == *"rank 1"* ]]
}

@test "usage is printed when a verb is missing its arguments" {
  run bash "$RERANK" chunks wren
  [ "$status" -eq 2 ]
  [[ "$output" == *"chorus-rerank"* ]]
}
