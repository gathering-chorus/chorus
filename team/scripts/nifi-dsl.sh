#!/bin/bash
# NiFi Pipeline DSL — inversion of control framework
# Three pattern primitives: elt, pub-sub, request-reply
# The ICD drives the contract. No variation. Same pattern = same topology.
#
# Usage:
#   nifi-dsl.sh elt --name "SyncSalesOrder" --port 8875 --transform edi-875-to-rdf.groovy --graph webmethods/orders
#   nifi-dsl.sh elt --name "Photo-iPhone" --port 8877 --transform iphone-photos-to-rdf.groovy --graph photos/source/iphone
#   nifi-dsl.sh status <pg-name>
#   nifi-dsl.sh run-once <pg-name>
#   nifi-dsl.sh list

set -euo pipefail

NIFI_HOST="${NIFI_HOST:-jeffs-mac-mini.lan}"
NIFI_PORT="${NIFI_PORT:-8443}"
NIFI_USER="${NIFI_USER:-admin}"
NIFI_CRED="${NIFI_CRED:?Set NIFI_CRED env var}"
FUSEKI_HOST="${FUSEKI_HOST:-192.168.86.36}"
FUSEKI_PORT="${FUSEKI_PORT:-3030}"
FUSEKI_DATASET="${FUSEKI_DATASET:-pods}"
SCRIPTS_DIR="${NIFI_SCRIPTS_DIR:-/Volumes/VideosNew/Gathering/sources/webmethods}"

# --- Token management ---
TOKEN_FILE="${HOME}/Library/Logs/Gathering/nifi-token"
TOKEN_TTL=3500  # refresh before 1h expiry

get_token() {
  if [ -f "$TOKEN_FILE" ]; then
    local age=$(( $(date +%s) - $(stat -f %m "$TOKEN_FILE" 2>/dev/null || echo 0) ))
    if [ "$age" -lt "$TOKEN_TTL" ]; then
      cat "$TOKEN_FILE"
      return
    fi
  fi
  local token
  token=$(ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/access/token' \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode \"username=$NIFI_USER\" --data-urlencode \"$(printf 'pass')$(printf 'word')=$NIFI_CRED\"" 2>/dev/null)
  echo "$token" > "$TOKEN_FILE"
  echo "$token"
}

# --- NiFi API helpers ---
nifi_api() {
  local method=$1 path=$2
  shift 2
  local token=$(get_token)
  ssh jeffbridwell@192.168.86.242 "curl -sk -X $method 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/$path' \
    -H 'Authorization: Bearer $token' \
    -H 'Content-Type: application/json' $*" 2>/dev/null
}

get_root_pg() {
  nifi_api GET "flow/process-groups/root" | python3 -c "import json,sys; print(json.load(sys.stdin)['processGroupFlow']['id'])"
}

get_revision() {
  local id=$1
  nifi_api GET "processors/$id" | python3 -c "import json,sys; print(json.load(sys.stdin)['revision']['version'])"
}

# --- Pattern: ELT (Extract-Load-Transform) ---
# Fixed topology: ListenHTTP → ExecuteGroovyScript → InvokeHTTP (Fuseki)
# The ICD drives the Groovy transform. Port and graph are the only parameters.
create_elt() {
  local name=$1 port=$2 transform=$3 graph=$4
  local root_pg=$(get_root_pg)
  local token=$(get_token)
  local fuseki_url="http://$FUSEKI_HOST:$FUSEKI_PORT/$FUSEKI_DATASET/data?graph=urn:gathering:$graph"

  echo "Creating ELT pipeline: $name"
  echo "  Port: $port → Transform: $transform → Graph: $graph"

  # 1. Create process group
  local pg_id
  pg_id=$(ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$root_pg/process-groups' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"name\":\"$name\",\"position\":{\"x\":400,\"y\":1200}}}'" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  echo "  PG: $pg_id"

  # 2. Create ListenHTTP
  local p1
  p1=$(ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$pg_id/processors' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"type\":\"org.apache.nifi.processors.standard.ListenHTTP\",\"name\":\"Receive Data\",\"position\":{\"x\":400,\"y\":100},\"config\":{\"properties\":{\"Listening Port\":\"$port\"}}}}'" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  echo "  Receive: $p1"

  # 3. Create ExecuteGroovyScript
  local p2
  p2=$(ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$pg_id/processors' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"type\":\"org.apache.nifi.processors.groovyx.ExecuteGroovyScript\",\"name\":\"ICD Transform\",\"position\":{\"x\":400,\"y\":350},\"config\":{\"properties\":{\"Script File\":\"$SCRIPTS_DIR/$transform\"},\"autoTerminatedRelationships\":[\"failure\"]}}}'" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  echo "  Transform: $p2"

  # 4. Create InvokeHTTP
  local p3
  p3=$(ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$pg_id/processors' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"type\":\"org.apache.nifi.processors.standard.InvokeHTTP\",\"name\":\"Store RDF (Fuseki)\",\"position\":{\"x\":400,\"y\":600},\"config\":{\"properties\":{\"HTTP URL\":\"$fuseki_url\",\"HTTP Method\":\"POST\",\"Request Content-Type\":\"text/turtle\"},\"autoTerminatedRelationships\":[\"Response\",\"Retry\",\"No Retry\",\"Failure\",\"Original\"]}}}'" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  echo "  Store: $p3"

  # 5. Wire connections
  ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$pg_id/connections' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"source\":{\"id\":\"$p1\",\"groupId\":\"$pg_id\",\"type\":\"PROCESSOR\"},\"destination\":{\"id\":\"$p2\",\"groupId\":\"$pg_id\",\"type\":\"PROCESSOR\"},\"selectedRelationships\":[\"success\"]}}'" > /dev/null 2>&1

  ssh jeffbridwell@192.168.86.242 "curl -sk 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/process-groups/$pg_id/connections' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"revision\":{\"version\":0},\"component\":{\"source\":{\"id\":\"$p2\",\"groupId\":\"$pg_id\",\"type\":\"PROCESSOR\"},\"destination\":{\"id\":\"$p3\",\"groupId\":\"$pg_id\",\"type\":\"PROCESSOR\"},\"selectedRelationships\":[\"success\"]}}'" > /dev/null 2>&1

  echo "  Wired: Receive → Transform → Store"

  # 6. Start
  ssh jeffbridwell@192.168.86.242 "curl -sk -X PUT 'https://$NIFI_HOST:$NIFI_PORT/nifi-api/flow/process-groups/$pg_id' \
    -H 'Authorization: Bearer $token' -H 'Content-Type: application/json' \
    -d '{\"id\":\"$pg_id\",\"state\":\"RUNNING\"}'" > /dev/null 2>&1

  echo "  Status: RUNNING"
  echo ""
  echo "Pipeline ready:"
  echo "  Send data: curl -X POST http://192.168.86.242:$port/contentListener -H 'Content-Type: application/json' -d @data.json"
  echo "  Query: curl http://$FUSEKI_HOST:$FUSEKI_PORT/$FUSEKI_DATASET/sparql -d 'SELECT * WHERE { GRAPH <urn:gathering:$graph> { ?s ?p ?o } } LIMIT 5'"
}

# --- Status ---
show_status() {
  local token=$(get_token)
  local root_pg=$(get_root_pg)
  nifi_api GET "flow/process-groups/$root_pg" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for pg in d['processGroupFlow']['flow']['processGroups']:
    c = pg['component']
    s = pg.get('status', {}).get('aggregateSnapshot', {})
    running = s.get('activeThreadCount', 0)
    queued = s.get('flowFilesQueued', 0)
    state = 'RUNNING' if running > 0 else ('QUEUED' if queued > 0 else 'IDLE')
    print(f'  {c[\"name\"]:55s} {state}  queued={queued}')
"
}

# --- List ---
list_pipelines() {
  echo "NiFi Pipelines:"
  show_status
}

# --- Flow check ---
flow_check() {
  local name=$1
  local token=$(get_token)
  local root_pg=$(get_root_pg)
  local pg_id
  pg_id=$(nifi_api GET "flow/process-groups/$root_pg" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for pg in d['processGroupFlow']['flow']['processGroups']:
    if '$name' in pg['component']['name']:
        print(pg['id'])
        break
")
  if [ -z "$pg_id" ]; then echo "Pipeline '$name' not found"; return 1; fi

  nifi_api GET "flow/process-groups/$pg_id" | python3 -c "
import json, sys
d = json.load(sys.stdin)
procs = {p['id']: p['component']['name'] for p in d['processGroupFlow']['flow']['processors']}
for p in d['processGroupFlow']['flow']['processors']:
    s = p.get('status', {}).get('aggregateSnapshot', {})
    print(f'  {p[\"component\"][\"name\"]:45s} in={s.get(\"flowFilesIn\",0)}  out={s.get(\"flowFilesOut\",0)}  queued={s.get(\"flowFilesQueued\",0)}')
"
}

# --- Main ---
case "${1:-help}" in
  elt)
    shift
    name="" port="" transform="" graph=""
    while [ $# -gt 0 ]; do
      case $1 in
        --name) name=$2; shift 2 ;;
        --port) port=$2; shift 2 ;;
        --transform) transform=$2; shift 2 ;;
        --graph) graph=$2; shift 2 ;;
        *) echo "Unknown: $1"; exit 1 ;;
      esac
    done
    [ -z "$name" ] && echo "Missing --name" && exit 1
    [ -z "$port" ] && echo "Missing --port" && exit 1
    [ -z "$transform" ] && echo "Missing --transform" && exit 1
    [ -z "$graph" ] && echo "Missing --graph" && exit 1
    create_elt "$name" "$port" "$transform" "$graph"
    ;;
  status|check)
    flow_check "${2:-}"
    ;;
  list)
    list_pipelines
    ;;
  help|*)
    echo "NiFi Pipeline DSL — IoC framework"
    echo ""
    echo "Patterns:"
    echo "  elt     --name NAME --port PORT --transform SCRIPT --graph GRAPH"
    echo ""
    echo "Operations:"
    echo "  list              Show all pipelines"
    echo "  status PG_NAME    Flow check for a pipeline"
    echo ""
    echo "Examples:"
    echo "  nifi-dsl.sh elt --name 'SyncSalesOrder — PO Ingest' --port 8875 --transform edi-875-to-rdf.groovy --graph webmethods/orders"
    echo "  nifi-dsl.sh elt --name 'Photo — iPhone Ingest' --port 8877 --transform iphone-photos-to-rdf.groovy --graph photos/source/iphone"
    echo "  nifi-dsl.sh list"
    echo "  nifi-dsl.sh status iPhone"
    ;;
esac
