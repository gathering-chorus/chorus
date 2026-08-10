#!/usr/bin/env bats
# @test-type: unit — hermetic. #3811: the crawler dispatches BESPOKE hydrators.
#
# A Hydratable class may declare chorus:hydratedBy "<script>" when its
# instances can't come from a generic file walk (chorus:Test needs the tagger's
# per-case parse + classification). The crawler runs that script as the class's
# hydration leg: exit 0 → crawler.graph.hydrated {class}, non-zero →
# crawler.graph.failed {class, reason=hydrator-exit-N} and a red run.
#
# Hermetic world: stub Fuseki (python http server answering 200), stub
# CHORUS_LOG capturing events to a file, tmp registry TTL, tmp CHORUS_ROOT.

setup() {
  export WORLD="$BATS_TEST_TMPDIR"
  export CHORUS_ROOT="$WORLD/root"
  mkdir -p "$CHORUS_ROOT"
  echo "x" > "$CHORUS_ROOT/one-file.txt"
  export HYDRATION_DB="$WORLD/index.db"
  export EVENTS="$WORLD/events.log"

  # stub chorus-log: append every emit's argv
  cat > "$WORLD/chorus-log" <<EOF
#!/bin/sh
echo "\$@" >> "$EVENTS"
EOF
  chmod +x "$WORLD/chorus-log"
  export CHORUS_LOG="$WORLD/chorus-log"

  # stub Fuseki: 200 to everything (ping, update, query). Double-forked with
  # all fds detached so bats never waits on it; pid recorded for teardown.
  cat > "$WORLD/stub.py" <<'PY'
import http.server, sys, os
class H(http.server.BaseHTTPRequestHandler):
    def _ok(self):
        body = b'{"results":{"bindings":[]}}'
        self.send_response(200); self.send_header('Content-Length', str(len(body)))
        self.end_headers(); self.wfile.write(body)
    do_GET = _ok
    do_POST = _ok
    def log_message(self, *a): pass
s = http.server.HTTPServer(('127.0.0.1', 0), H)
open(sys.argv[1], 'w').write(str(s.server_address[1]))
open(sys.argv[2], 'w').write(str(os.getpid()))
s.serve_forever()
PY
  ( python3 "$WORLD/stub.py" "$WORLD/port" "$WORLD/stub.pid" >/dev/null 2>&1 </dev/null & ) 3>&-
  for _ in $(seq 1 50); do [ -s "$WORLD/port" ] && break; sleep 0.1; done
  PORT=$(cat "$WORLD/port")
  export FUSEKI_BASE="http://127.0.0.1:$PORT/pods"
  export FUSEKI_UPDATE="http://127.0.0.1:$PORT/pods/update"
  export FUSEKI_PING="http://127.0.0.1:$PORT/ping"

  # registry TTL: File (generic) + Test (bespoke hydrator, filled per test)
  export TTL="$WORLD/registry.ttl"
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../scripts" && pwd)/crawler-hydrate-graph.sh"
}

teardown() {
  [ -s "$WORLD/stub.pid" ] && kill "$(cat "$WORLD/stub.pid")" 2>/dev/null || true
}

write_ttl() { # $1 = hydrator line for chorus:Test ('' = none)
  cat > "$TTL" <<EOF
chorus:Hydratable a owl:Class ;
    rdfs:comment "marker" .
chorus:File a owl:Class ;
    rdfs:subClassOf chorus:Hydratable ;
    chorus:hydratesFromSource "**/*" .
chorus:filePath a owl:DatatypeProperty ;
    rdfs:domain chorus:File ;
    chorus:writeOwner chorus:crawler .
chorus:Test a owl:Class ;
    rdfs:subClassOf chorus:Hydratable ;
    chorus:hydratesFromSource "platform/**tests**" ;
    $1
    chorus:mintKey "x" .
chorus:testName a owl:DatatypeProperty ;
    rdfs:domain chorus:Test ;
    chorus:writeOwner chorus:crawler .
EOF
}

@test "bespoke hydrator runs and a clean exit emits crawler.graph.hydrated for the class" {
  cat > "$WORLD/hydrator-ok.sh" <<EOF
#!/bin/sh
echo "tests: 7"
exit 0
EOF
  chmod +x "$WORLD/hydrator-ok.sh"
  write_ttl "chorus:hydratedBy \"$WORLD/hydrator-ok.sh\" ;"
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  grep -q "crawler.graph.hydrated .* class=chorus:Test" "$EVENTS"
  ! grep -q "class=chorus:Test.*no-hydrator-implementation-yet" "$EVENTS"
}

@test "NEGATIVE PROOF (#3734): a failing hydrator emits crawler.graph.failed and reds the run" {
  cat > "$WORLD/hydrator-bad.sh" <<EOF
#!/bin/sh
echo "boom" >&2
exit 3
EOF
  chmod +x "$WORLD/hydrator-bad.sh"
  write_ttl "chorus:hydratedBy \"$WORLD/hydrator-bad.sh\" ;"
  run bash "$SCRIPT"
  [ "$status" -eq 1 ]
  grep -q "crawler.graph.failed .* class=chorus:Test reason=hydrator-exit-3" "$EVENTS"
}

@test "a hydratable class with NO hydrator keeps the loud skip (never silent)" {
  write_ttl ""
  run bash "$SCRIPT"
  grep -q "crawler.graph.skipped .* class=chorus:Test reason=no-hydrator-implementation-yet" "$EVENTS"
}
