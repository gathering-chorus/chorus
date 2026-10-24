#!/usr/bin/env bats
# @test-type: integration — boots a scratch Fuseki on disk (the live jar, TDB2 like prod, a free port); never touches :3030.
# @domain: persistence
# @card: #4717
# @owner: silas
# wipe-guard: exempt — every write goes to the scratch Fuseki this file boots on a free port, never :3030
#
# #4717 reopen — platform/scripts/shacl-decimal-canon and platform/scripts/shacl-retype.
# What Jeff sees: a sensor reading edited twice holds one value, not two. On
# 10-24 a reading held 7.0 and 8.0 and the crawler-out-of-sync check crashed:
# TDB2 keeps a decimal written "7" or "7.00" under that spelling, reads it back
# as 7.0, and so `DELETE WHERE { s p ?o }` cannot delete it. The bug is TDB2's,
# so this file boots TDB2; an in-memory dataset does not show it.

ROOT="$BATS_TEST_DIRNAME/../.."
FUSEKI_HOME="${FUSEKI_HOME:-$HOME/.gathering/data/fuseki-5.1.0}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk/bin/java}"
C="https://jeffbridwell.com/chorus#"
SH="http://www.w3.org/ns/shacl#"
D="<http://www.w3.org/2001/XMLSchema#decimal>"
G="urn:chorus:domains:heralds"

setup() {
  [ -r "$FUSEKI_HOME/fuseki-server.jar" ] || skip "no Fuseki jar at $FUSEKI_HOME"
  T="$(mktemp -d)"
  export FUSEKI_ADMIN_PASSWORD= FUSEKI_ADMIN_USER=
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
  mkdir -p "$T/base" "$T/db"
  cat > "$T/base/config.ttl" <<TTL
@prefix fuseki: <http://jena.apache.org/fuseki#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix tdb2: <http://jena.apache.org/2016/tdb#> .
[] rdf:type fuseki:Server ; fuseki:services ( <#svc> ) .
<#svc> a fuseki:Service ; fuseki:name "pods" ;
   fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name "query" ] ;
   fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name "update" ] ;
   fuseki:dataset [ a tdb2:DatasetTDB2 ; tdb2:location "$T/db" ] .
TTL
  (cd "$T/base" && FUSEKI_HOME="$FUSEKI_HOME" FUSEKI_BASE="$T/base" exec "$JAVA" -Xmx256m \
     -cp "$FUSEKI_HOME/fuseki-server.jar" org.apache.jena.fuseki.cmd.FusekiCmd \
     --config="$T/base/config.ttl" --port="$PORT" > "$T/server.log" 2>&1) &
  PID=$!
  for _ in $(seq 1 40); do curl -sf -o /dev/null --max-time 1 "localhost:$PORT/\$/ping" && break; sleep 0.5; done
  export FUSEKI_QUERY="http://localhost:$PORT/pods/query" FUSEKI_UPDATE="http://localhost:$PORT/pods/update"
  export SHACL_DECIMAL_BACKUP="$T/backup.tsv" SHACL_RETYPE_BACKUP="$T/retype.nt"
  upd "INSERT DATA { GRAPH <urn:chorus:ontology> {
    <${C}SensorReadingShape> a <${SH}NodeShape> ; <${SH}targetClass> <${C}SensorReading> ;
      <${SH}property> [ <${SH}path> <${C}readingValue> ; <${SH}datatype> $D ] } }"
}

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
  rm -rf "$T"
}

upd() { curl -sf -o /dev/null --max-time 10 "localhost:$PORT/pods/update" --data-urlencode "update=$1"; }
q() { curl -s --max-time 10 "localhost:$PORT/pods/query" -H 'Accept: text/tab-separated-values' --data-urlencode "query=$1" | sed 1d; }
reading() { upd "INSERT DATA { GRAPH <$G> { <${C}r-$1> a <${C}SensorReading> ; <${C}readingValue> $2 } }"; }
# what a replace leaves: delete every value of the field, then count what is left
replace_leaves() {
  upd "DELETE WHERE { GRAPH <$G> { <${C}r-$1> <${C}readingValue> ?o } } ; INSERT DATA { GRAPH <$G> { <${C}r-$1> <${C}readingValue> \"8.0\"^^$D } }"
  q "SELECT (COUNT(?o) AS ?n) WHERE { GRAPH <$G> { <${C}r-$1> <${C}readingValue> ?o } }"
}

@test "negative proof: this store keeps a decimal spelled 7 through a replace (the 10-24 bug is real here)" {
  reading a "\"7\"^^$D"
  [ "$(replace_leaves a)" = 2 ]
}

@test "a dry run counts each decimal a replace cannot delete and writes nothing" {
  reading a "\"7\"^^$D"; reading b "\"7.00\"^^$D"; reading c "\"7.50\"^^$D"; reading d "\"0.50\"^^$D"
  reading ok "\"7.5\"^^$D"; reading plain "\"7\""
  run "$ROOT/platform/scripts/shacl-decimal-canon"
  [ "$status" -eq 0 ] && [[ "$output" == *"4 decimal(s) a replace cannot delete"* ]] && [[ "$output" == *"dry run, nothing written"* ]] || { echo "$output"; return 1; }
  [ ! -e "$T/backup.tsv" ]
  [ "$(replace_leaves a)" = 2 ]
}

@test "apply backs up the list, rewrites each canonical, and then a replace leaves one value" {
  reading a "\"7\"^^$D"; reading b "\"7.00\"^^$D"; reading c "\"7.50\"^^$D"; reading d "\"0.50\"^^$D"; reading e "\"-12.300\"^^$D"
  reading ok "\"7.5\"^^$D"
  run "$ROOT/platform/scripts/shacl-decimal-canon" --apply
  [ "$status" -eq 0 ] && [[ "$output" == *"rewrote 5 of 5; 0 left"* ]] || { echo "$output"; return 1; }
  [ "$(grep -c . "$T/backup.tsv")" = 5 ]
  [ "$(q "SELECT ?v WHERE { GRAPH <$G> { <${C}r-c> <${C}readingValue> ?o } BIND(STR(?o) AS ?v) }")" = '"7.5"' ]
  [ "$(q "SELECT ?v WHERE { GRAPH <$G> { <${C}r-e> <${C}readingValue> ?o } BIND(STR(?o) AS ?v) }")" = '"-12.3"' ]
  for r in a b c d e ok; do [ "$(replace_leaves $r)" = 1 ] || { echo "r-$r kept its old value"; return 1; }; done
}

@test "negative proof: a store that does not answer writes nothing" {
  FUSEKI_QUERY="http://localhost:1/pods/query" FUSEKI_UPDATE="http://localhost:1/pods/update" run "$ROOT/platform/scripts/shacl-decimal-canon" --apply
  [ "$status" -eq 1 ] && [ ! -e "$T/backup.tsv" ] || { echo "$output"; return 1; }
}

@test "retype writes a plain decimal canonical, so a replace after it leaves one value" {
  reading a '"7"'; reading b '"7.50"'
  run "$ROOT/platform/scripts/shacl-retype" --apply
  [ "$status" -eq 0 ] && [[ "$output" == *"retyped 2 of 2; 0 left"* ]] || { echo "$output"; return 1; }
  [ "$(q "SELECT ?v WHERE { GRAPH <$G> { <${C}r-a> <${C}readingValue> ?o } BIND(DATATYPE(?o) AS ?v) }")" = "$D" ]
  run "$ROOT/platform/scripts/shacl-decimal-canon"
  [[ "$output" == *"0 decimal(s) a replace cannot delete"* ]] || { echo "$output"; return 1; }
  [ "$(replace_leaves a)" = 1 ] && [ "$(replace_leaves b)" = 1 ]
}
