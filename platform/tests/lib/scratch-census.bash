# scratch-census.bash — a scratch Fuseki on a free port as the "store" for shacl-census tests
# (#4717, #4920). Never touches :3030. Loaded by 4717-shacl-metrics.bats and 4920-baseline-alert.bats.
# wipe-guard: exempt — every write goes to the scratch Fuseki this file boots on a free port, never :3030

ROOT="$BATS_TEST_DIRNAME/../.."
FUSEKI_HOME="${FUSEKI_HOME:-$HOME/.gathering/data/fuseki-5.1.0}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk/bin/java}"
C="https://jeffbridwell.com/chorus#"
SH="http://www.w3.org/ns/shacl#"

setup() {
  [ -r "$FUSEKI_HOME/fuseki-server.jar" ] || skip "no Fuseki jar at $FUSEKI_HOME"
  T="$(mktemp -d)"
  export TEXTFILE_DIR="$T/textfile" SHACL_CENSUS_STATE="$T/state.json" SHACL_CENSUS_HEAP=256m
  export FUSEKI_ADMIN_PASSWORD= FUSEKI_ADMIN_USER=
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
  mkdir -p "$T/base"
  cat > "$T/base/config.ttl" <<'TTL'
@prefix fuseki: <http://jena.apache.org/fuseki#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .
[] rdf:type fuseki:Server ; fuseki:services ( <#svc> ) .
<#svc> a fuseki:Service ; fuseki:name "pods" ;
   fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name "query" ] ;
   fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name "update" ] ;
   fuseki:dataset [ a ja:MemoryDataset ] .
TTL
  (cd "$T/base" && FUSEKI_HOME="$FUSEKI_HOME" FUSEKI_BASE="$T/base" exec "$JAVA" -Xmx256m \
     -cp "$FUSEKI_HOME/fuseki-server.jar" org.apache.jena.fuseki.cmd.FusekiCmd \
     --config="$T/base/config.ttl" --port="$PORT" > "$T/server.log" 2>&1) &
  PID=$!
  for _ in $(seq 1 40); do curl -sf -o /dev/null --max-time 1 "localhost:$PORT/\$/ping" && break; sleep 0.5; done
  export FUSEKI_QUERY="http://localhost:$PORT/pods/query"
  upd "INSERT DATA { GRAPH <urn:chorus:ontology> {
    <${C}AlertShape> a <${SH}NodeShape> ; <${SH}targetClass> <${C}Alert> ;
      <${SH}property> [ <${SH}path> <${C}owner> ; <${SH}minCount> 1 ] ;
      <${SH}property> [ <${SH}path> <${C}muted> ; <${SH}datatype> <http://www.w3.org/2001/XMLSchema#boolean> ] } }"
}

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
  rm -rf "$T"
}

upd() { curl -sf -o /dev/null --max-time 10 "localhost:$PORT/pods/update" --data-urlencode "update=$1"; }
gauge() { grep -E "^$1 " "$2" | awk '{print $2}'; }
alerts() {  # n alerts with `bad` of them broken twice (no owner, muted "maybe")
  upd "DROP SILENT GRAPH <urn:chorus:domains:alerts>"
  local i
  for i in $(seq 1 "$1"); do
    if [ "$i" -le "$2" ]; then upd "INSERT DATA { GRAPH <urn:chorus:domains:alerts> { <${C}alert-$i> a <${C}Alert> ; <${C}muted> \"maybe\" } }"
    else upd "INSERT DATA { GRAPH <urn:chorus:domains:alerts> { <${C}alert-$i> a <${C}Alert> ; <${C}owner> \"silas\" } }"; fi
  done
}
