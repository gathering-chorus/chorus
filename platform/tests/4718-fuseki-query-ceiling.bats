#!/usr/bin/env bats
# @test-type: integration — boots a scratch Fuseki (the live jar, an in-memory dataset, a free port); never touches :3030 or the store.
# @domain: persistence — Fuseki is the store; the ceiling keeps one query from taking the door down (#4718)
#
# #4718 — Fuseki has no cancel on client disconnect. 10-22 07:02: one unbounded
# CONSTRUCT ran 7+ minutes after its curl died and the door answered 502 until
# a kickstart. The canonical server config carries a query ceiling, and the
# ceiling really kills a runaway inside Fuseki. Negative proof: the same
# runaway with the ceiling line removed is still running after the client quits.

ROOT="$BATS_TEST_DIRNAME/../.."
CANON="$ROOT/platform/scripts/launchagents-canonical/fuseki-config-server.ttl.in"
FUSEKI_HOME="${FUSEKI_HOME:-$HOME/.gathering/data/fuseki-5.1.0}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk/bin/java}"
# ten patterns over ten triples: 10^10 rows, never finishes on its own
RUNAWAY='SELECT (COUNT(*) AS ?n) WHERE { ?a ?p1 ?x1 . ?b ?p2 ?x2 . ?c ?p3 ?x3 . ?d ?p4 ?x4 . ?e ?p5 ?x5 . ?f ?p6 ?x6 . ?g ?p7 ?x7 . ?h ?p8 ?x8 . ?i ?p9 ?x9 . ?j ?p10 ?x10 }'

setup() {
  [ -r "$FUSEKI_HOME/fuseki-server.jar" ] || skip "no Fuseki jar at $FUSEKI_HOME"
  T="$(mktemp -d)"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
}

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
  rm -rf "$T"
}

# boot <server-config>: the canonical server block, plus one in-memory service
boot() {
  mkdir -p "$T/base"
  { cat "$1"; cat <<'TTL'
<#svc> a fuseki:Service ; fuseki:name "t" ;
   fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name "query" ] ;
   fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name "update" ] ;
   fuseki:dataset [ a ja:MemoryDataset ] .
TTL
  } | sed 's/^\[\] rdf:type fuseki:Server ;/[] rdf:type fuseki:Server ; fuseki:services ( <#svc> ) ;/' > "$T/base/config.ttl"
  (cd "$T/base" && FUSEKI_HOME="$FUSEKI_HOME" FUSEKI_BASE="$T/base" exec "$JAVA" -Xmx256m \
     -cp "$FUSEKI_HOME/fuseki-server.jar" org.apache.jena.fuseki.cmd.FusekiCmd \
     --config="$T/base/config.ttl" --port="$PORT" > "$T/server.log" 2>&1) &
  PID=$!
  for _ in $(seq 1 40); do curl -sf -o /dev/null --max-time 1 "localhost:$PORT/\$/ping" && break; sleep 0.5; done
  curl -sf -o /dev/null --max-time 5 "localhost:$PORT/t/update" \
    --data-urlencode 'update=INSERT DATA { <a> <p> 1 . <b> <p> 2 . <c> <p> 3 . <d> <p> 4 . <e> <p> 5 . <f> <p> 6 . <g> <p> 7 . <h> <p> 8 . <i> <p> 9 . <j> <p> 10 }'
}

@test "the canonical server config declares a query ceiling above the heaviest real query (74 s) and at most 120 s" {
  ms=$(grep -oE 'arq:queryTimeout" *; *ja:cxtValue "[0-9]+"' "$CANON" | grep -oE '[0-9]+"$' | tr -d '"')
  [ -n "$ms" ] || { echo "no arq:queryTimeout in $CANON"; false; }
  [ "$ms" -gt 74000 ] && [ "$ms" -le 120000 ] || { echo "ceiling $ms ms is outside (74000, 120000]"; false; }
}

@test "the ceiling kills a planted runaway inside Fuseki (canonical file, ceiling scaled to 3 s)" {
  sed 's/ja:cxtValue "[0-9]*"/ja:cxtValue "3000"/' "$CANON" > "$T/server.ttl"
  boot "$T/server.ttl"
  run curl -s -o /dev/null --max-time 30 -w '%{http_code}' "localhost:$PORT/t/query" --data-urlencode "query=$RUNAWAY"
  [ "$output" = 503 ] || { echo "got '$output', want 503 from the ceiling"; cat "$T/server.log"; false; }
  grep -qE '503 Service Unavailable \([0-9.]+ s\)' "$T/server.log"
}

@test "negative proof: without the ceiling line the runaway is still running after the client gives up" {
  # the same file with only the ceiling line swapped for a comment triple
  sed 's/^ *ja:context \[ ja:cxtName "arq:queryTimeout".*$/   rdfs:comment "no ceiling" ./' "$CANON" > "$T/server.ttl"
  ! grep -q queryTimeout "$T/server.ttl" || return 1
  boot "$T/server.ttl"
  run curl -s -o /dev/null --max-time 6 -w '%{http_code}' "localhost:$PORT/t/query" --data-urlencode "query=$RUNAWAY"
  [ "$output" = 000 ] || { echo "got '$output': the query ended on its own"; false; }
  sleep 4
  # the query's id was logged as started and never as finished
  id=$(grep -oE '\[[0-9]+\] POST .*/t/query' "$T/server.log" | grep -oE '^\[[0-9]+\]' | tail -1)
  [ -n "$id" ] && ! grep -qF "$id 200" "$T/server.log" && ! grep -qF "$id 503" "$T/server.log"
}
