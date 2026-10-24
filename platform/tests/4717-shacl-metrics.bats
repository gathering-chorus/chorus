#!/usr/bin/env bats
# @test-type: integration — boots a scratch Fuseki as the "store" (the live jar, in memory, a free port); never touches :3030.
# @domain: persistence
# @card: #4717
# @owner: silas
# wipe-guard: exempt — every write goes to the scratch Fuseki this file boots on a free port, never :3030
#
# #4717 AC4/AC5 — platform/scripts/shacl-bnodes and platform/scripts/shacl-census
# write the gauges node-exporter serves. What Jeff sees: an emptied shared blank
# node in the shapes turns the gauge red within the hour; the store's violation
# count is a ratchet that only goes down; a census that cannot read the store
# writes nothing, so its file goes stale and alerts, never a clean-looking 0.

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

@test "the blank-node gauge reads 0 on healthy shapes" {
  run "$ROOT/platform/scripts/shacl-bnodes"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(gauge athena_shared_bnodes_empty "$TEXTFILE_DIR/athena_shacl_bnodes.prom")" = 0 ]
}

# AC5 — the 10-18 class: two shapes share a property node and a land empties it
@test "negative proof: an emptied shared blank node turns the gauge to 1" {
  upd "INSERT DATA { GRAPH <urn:chorus:ontology> { <${C}AShape> <${SH}property> _:gone . <${C}BShape> <${SH}property> _:gone . <${C}CShape> <${SH}property> _:solo } }"
  run "$ROOT/platform/scripts/shacl-bnodes"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(gauge athena_shared_bnodes_empty "$TEXTFILE_DIR/athena_shacl_bnodes.prom")" = 1 ]
  [ "$(gauge athena_empty_bnodes "$TEXTFILE_DIR/athena_shacl_bnodes.prom")" = 2 ]
}

@test "negative proof: a store that does not answer writes no gauge (it goes stale, never 0)" {
  FUSEKI_QUERY="http://localhost:1/pods/query" run "$ROOT/platform/scripts/shacl-bnodes"
  [ "$status" -eq 1 ] && [ ! -e "$TEXTFILE_DIR/athena_shacl_bnodes.prom" ] || { echo "$output"; return 1; }
}

@test "the census counts every violation by Jena, and the baseline only goes down" {
  alerts 5 2
  run "$ROOT/platform/scripts/shacl-census"
  [ "$status" -eq 0 ] || { echo "$output"; cat "$T"/server.log | tail -5; return 1; }
  f="$TEXTFILE_DIR/athena_shacl.prom"
  [ "$(gauge athena_shacl_violations "$f")" = 4 ] && [ "$(gauge athena_shacl_violating_rows "$f")" = 2 ] || { cat "$f"; return 1; }
  [ "$(gauge athena_shacl_violations_baseline "$f")" = 4 ]
  alerts 5 3   # a rise: the count goes up, the baseline stays
  run "$ROOT/platform/scripts/shacl-census"
  [ "$(gauge athena_shacl_violations "$f")" = 6 ] && [ "$(gauge athena_shacl_violations_baseline "$f")" = 4 ] || { cat "$f"; return 1; }
  alerts 5 1   # a fix: the baseline follows it down
  run "$ROOT/platform/scripts/shacl-census"
  [ "$(gauge athena_shacl_violations "$f")" = 2 ] && [ "$(gauge athena_shacl_violations_baseline "$f")" = 2 ] || { cat "$f"; return 1; }
}

@test "negative proof: a census that cannot read the store writes nothing" {
  alerts 2 1
  FUSEKI_QUERY="http://localhost:1/pods/query" run "$ROOT/platform/scripts/shacl-census"
  [ "$status" -ne 0 ] && [ ! -e "$TEXTFILE_DIR/athena_shacl.prom" ] && [ ! -e "$SHACL_CENSUS_STATE" ] || { echo "$output"; return 1; }
}

# AC6 — platform/scripts/shacl-retype: the datatype changes, the value never does
@test "retype: a dry run counts and writes nothing; apply retypes what matches, backs it up, and leaves the rest" {
  upd "INSERT DATA { GRAPH <urn:chorus:ontology> { <${C}AlertShape> <${SH}property> [ <${SH}path> <${C}rank> ; <${SH}datatype> <http://www.w3.org/2001/XMLSchema#integer> ] } }"
  upd "INSERT DATA { GRAPH <urn:chorus:domains:alerts> { <${C}a1> a <${C}Alert> ; <${C}rank> \"38\" ; <${C}muted> \"true\" ; <${C}owner> \"7\" . <${C}a2> a <${C}Alert> ; <${C}rank> \"high\" ; <${C}muted> \"maybe\" } }"
  export FUSEKI_UPDATE="http://localhost:$PORT/pods/update" SHACL_RETYPE_BACKUP="$T/backup.nt"
  run "$ROOT/platform/scripts/shacl-retype"
  [ "$status" -eq 0 ] && [[ "$output" == *"2 literal(s) can be retyped"* ]] && [[ "$output" == *"dry run, nothing written"* ]] || { echo "$output"; return 1; }
  [ ! -e "$T/backup.nt" ]
  run "$ROOT/platform/scripts/shacl-retype" --apply
  [ "$status" -eq 0 ] && [[ "$output" == *"retyped 2 of 2; 0 left"* ]] || { echo "$output"; return 1; }
  [ "$(grep -c . "$T/backup.nt")" = 2 ]
  q() { curl -s --max-time 10 "localhost:$PORT/pods/query" -H 'Accept: text/tab-separated-values' --data-urlencode "query=$1" | sed -n 2p; }
  [ "$(q "SELECT ?v WHERE { GRAPH ?g { <${C}a1> <${C}rank> ?o } BIND(DATATYPE(?o) AS ?v) }")" = "<http://www.w3.org/2001/XMLSchema#integer>" ]
  [ "$(q "SELECT ?v WHERE { GRAPH ?g { <${C}a1> <${C}rank> ?o } BIND(STR(?o) AS ?v) }")" = '"38"' ]
  # negative: what does not fit its datatype, and a string field, are untouched
  [ "$(q "SELECT ?v WHERE { GRAPH ?g { <${C}a2> <${C}rank> ?o } BIND(DATATYPE(?o) AS ?v) }")" = "<http://www.w3.org/2001/XMLSchema#string>" ]
  [ "$(q "SELECT ?v WHERE { GRAPH ?g { <${C}a2> <${C}muted> ?o } BIND(DATATYPE(?o) AS ?v) }")" = "<http://www.w3.org/2001/XMLSchema#string>" ]
  [ "$(q "SELECT ?v WHERE { GRAPH ?g { <${C}a1> <${C}owner> ?o } BIND(DATATYPE(?o) AS ?v) }")" = "<http://www.w3.org/2001/XMLSchema#string>" ]
}

# The two jobs: proving/config/launchagents/com.chorus.shacl-census.plist (nightly)
# and proving/config/launchagents/com.chorus.shacl-bnodes.plist (hourly) run the
# scripts in this repo, on the schedules the alert rules assume (26 h and 2 h).
@test "the census plist runs shacl-census nightly and the blank-node plist runs shacl-bnodes hourly" {
  LA="$ROOT/proving/config/launchagents"
  for job in shacl-census shacl-bnodes; do
    plutil -lint "$LA/com.chorus.$job.plist" >/dev/null || return 1
    script=$(plutil -extract ProgramArguments.1 raw "$LA/com.chorus.$job.plist")
    [ "$script" = "$HOME/CascadeProjects/chorus/platform/scripts/$job" ] || { echo "$job runs $script"; return 1; }
    [ -x "$ROOT/platform/scripts/$job" ] || { echo "$job: no such script in the repo"; return 1; }
  done
  [ "$(plutil -extract StartCalendarInterval.Hour raw "$LA/com.chorus.shacl-census.plist")" = 4 ] || return 1
  [ "$(plutil -extract StartInterval raw "$LA/com.chorus.shacl-bnodes.plist")" = 3600 ] || return 1
}
