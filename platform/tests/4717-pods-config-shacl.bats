#!/usr/bin/env bats
# @test-type: integration — boots a scratch Fuseki from the canonical /pods config (its TDB2 location moved to a temp dir, a free port); never touches :3030.
# @domain: persistence
# @card: #4717
# @owner: silas
#
# #4717 — platform/scripts/launchagents-canonical/fuseki-config-pods.ttl gives
# /pods the SHACL service the DAL's gate posts to, and keeps the union default
# graph (#3732) the live file has had since 2026-07; the canonical copy had
# dropped it, so deploying it would have taken union away.

ROOT="$BATS_TEST_DIRNAME/../.."
CANON="$ROOT/platform/scripts/launchagents-canonical/fuseki-config-pods.ttl"
FUSEKI_HOME="${FUSEKI_HOME:-$HOME/.gathering/data/fuseki-5.1.0}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk/bin/java}"

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
  if [ -n "${T:-}" ]; then rm -rf "$T"; fi
}

@test "the canonical /pods config declares a shacl endpoint and the union default graph" {
  grep -qE 'fuseki:operation fuseki:shacl ; +fuseki:name "shacl"' "$CANON"
  grep -qE '^ *tdb2:unionDefaultGraph true' "$CANON"
}

@test "a Fuseki started from it answers SHACL at /pods/shacl (and the copy without the line does not)" {
  [ -r "$FUSEKI_HOME/fuseki-server.jar" ] || skip "no Fuseki jar"
  T="$(mktemp -d)"
  boot() {
    PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
    sed "s|tdb2:location \"[^\"]*\"|tdb2:location \"$T/tdb-$PORT\"|" "$1" > "$T/config-$PORT.ttl"
    (cd "$T" && FUSEKI_HOME="$FUSEKI_HOME" FUSEKI_BASE="$T" exec "$JAVA" -Xmx256m -cp "$FUSEKI_HOME/fuseki-server.jar" \
       org.apache.jena.fuseki.cmd.FusekiCmd --config="$T/config-$PORT.ttl" --port="$PORT" > "$T/server-$PORT.log" 2>&1) &
    PID=$!
    for _ in $(seq 1 40); do curl -sf -o /dev/null --max-time 1 "localhost:$PORT/\$/ping" && break; sleep 0.5; done
    curl -sf -o /dev/null --max-time 5 "localhost:$PORT/pods/update" --data-urlencode 'update=INSERT DATA { GRAPH <urn:g> { <urn:a> a <urn:C> } }'
  }
  printf '%s\n' '@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:S> a sh:NodeShape ; sh:targetClass <urn:C> ; sh:property [ sh:path <urn:p> ; sh:minCount 1 ] .' > "$T/shapes.ttl"
  boot "$CANON"
  run curl -s --max-time 20 -X POST -H 'Content-Type: text/turtle' -H 'Accept: application/n-triples' --data-binary "@$T/shapes.ttl" "localhost:$PORT/pods/shacl?graph=urn:g"
  [[ "$output" == *"MinCountConstraintComponent"* ]] || { echo "$output"; cat "$T"/server-*.log | tail -5; return 1; }
  kill "$PID"; wait "$PID" 2>/dev/null || true
  grep -v 'fuseki:operation fuseki:shacl' "$CANON" > "$T/no-shacl.ttl"
  boot "$T/no-shacl.ttl"
  run curl -s -o /dev/null -w '%{http_code}' --max-time 20 -X POST -H 'Content-Type: text/turtle' --data-binary "@$T/shapes.ttl" "localhost:$PORT/pods/shacl?graph=urn:g"
  # Fuseki answers 405 here (measured): the path is no service
  [ "$output" != 200 ] || { echo "without the endpoint: $output"; return 1; }
}
