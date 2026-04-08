#!/usr/bin/env bash
# harvest-media.sh — Harvest media collection from MongoDB to Fuseki RDF
# Card: #376 | Architecture: volume-sharded collection graphs
# See: architect/briefs/2026-02-23-images-api-harvest-architecture.md
set -euo pipefail

SECONDARY="jeffbridwell@192.168.86.242"
MONGOSH="/opt/homebrew/bin/mongosh"
DB="media"
FUSEKI_URL="http://localhost:3031"
DATASET="pods"
GRAPH_BASE="https://jeffbridwell.com/pods/jeff/media"
HARVEST_DIR="/Volumes/VideosNew/Gathering/Media/generated/harvest"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHUNK_LIMIT=100000  # items per upload chunk (200K caused Fuseki OOM)

# Chorus log (optional)
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

# Fuseki auth from app .env
ENV_FILE="$SCRIPT_DIR/../../jeff-bridwell-personal-site/.env"
FUSEKI_PW=""
if [ -f "$ENV_FILE" ]; then
  FUSEKI_PW=$(grep -m1 '^FUSEKI_ADMIN_PASSWORD=' "$ENV_FILE" | cut -d= -f2 | tr -d '"' 2>/dev/null || true)
fi

mkdir -p "$HARVEST_DIR"

# Notify on unhandled failure
_current_domain=""
trap 'if [ -n "$_current_domain" ]; then notify_harvest "$_current_domain" "failed" 0 "" "Script exited unexpectedly"; fi' EXIT

log() { echo "$(date '+%H:%M:%S') $*"; }

chorus_event() {
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "$@" 2>/dev/null || true
}

notify_harvest() {
  local domain="$1" result="$2" items="${3:-0}" duration="${4:-}" error="${5:-}"
  curl -s -X POST http://localhost:9095/harvest \
    -H "Content-Type: application/json" \
    -d "{\"domain\":\"$domain\",\"result\":\"$result\",\"items\":$items,\"duration\":\"$duration\",\"error\":\"$(echo "$error" | head -c 120)\"}" \
    --max-time 5 >/dev/null 2>&1 || true
}

encode_uri() {
  python3 -c "import urllib.parse; print(urllib.parse.quote('$1', safe=''))"
}

# --- Commands ---

cmd_list() {
  log "Fetching volume distribution from MongoDB..."
  ssh -o ConnectTimeout=10 "$SECONDARY" "$MONGOSH --quiet $DB --file /dev/stdin" <<'EOF'
db.content.aggregate([
  {$project: {vol: {$arrayElemAt: [{$split: ["$file_path", "/"]}, 2]}}},
  {$group: {_id: "$vol", count: {$sum: 1}}},
  {$sort: {count: -1}}
], {allowDiskUse: true}).forEach(r => print(r._id + "\t" + r.count));
print("---");
print("models\t" + db.models.countDocuments());
EOF
}

upload_ttl() {
  local graph_uri="$1" ttl_file="$2" method="${3:-PUT}"
  local encoded=$(encode_uri "$graph_uri")
  local size=$(wc -c < "$ttl_file" | tr -d ' ')
  local auth_flag=""
  [ -n "$FUSEKI_PW" ] && auth_flag="-u admin:$FUSEKI_PW"

  local code
  code=$(curl -s -o "$HARVEST_DIR/_upload.log" -w "%{http_code}" \
    -X "$method" \
    "$FUSEKI_URL/$DATASET/data?graph=$encoded" \
    -H "Content-Type: text/turtle" \
    $auth_flag \
    -T "$ttl_file" \
    --max-time 600)

  if [ "$code" -ge 200 ] && [ "$code" -lt 300 ]; then
    log "  Uploaded ($code, $(( size / 1024 )) KB)"
    return 0
  else
    log "  Upload FAILED ($code)"
    cat "$HARVEST_DIR/_upload.log" >&2
    return 1
  fi
}

delete_graph() {
  local graph_uri="$1"
  local encoded=$(encode_uri "$graph_uri")
  local auth_flag=""
  [ -n "$FUSEKI_PW" ] && auth_flag="-u admin:$FUSEKI_PW"

  curl -s -o /dev/null -w "" \
    -X DELETE "$FUSEKI_URL/$DATASET/data?graph=$encoded" \
    $auth_flag 2>/dev/null || true
}

get_volume_count() {
  local vol="$1"
  ssh -o ConnectTimeout=10 "$SECONDARY" "$MONGOSH --quiet $DB --file /dev/stdin" <<COUNTEOF 2>/dev/null | tail -1
print(db.content.countDocuments({file_path: {\$regex: '^/[^/]+/$vol/'}}));
COUNTEOF
}

harvest_volume() {
  local vol="$1"
  local graph_uri="$GRAPH_BASE/$vol"
  local vol_start=$(date +%s)

  log "=== Volume: $vol ==="
  _current_domain="media/$vol"
  chorus_event harvest.pipeline.started silas "collection=$vol" "type=media"

  # Get count
  local total
  total=$(get_volume_count "$vol")
  log "  Items: $total"

  # Delete existing graph
  delete_graph "$graph_uri"

  if [ "$total" -le "$CHUNK_LIMIT" ]; then
    # Single upload
    local ttl_file="$HARVEST_DIR/${vol}.ttl"
    log "  Exporting (single pass)..."
    local t0=$(date +%s)

    printf "var VOLUME = '%s';\n" "$vol" > "$HARVEST_DIR/_header.js"
    cat "$HARVEST_DIR/_header.js" "$SCRIPT_DIR/harvest-media-export.js" | \
      ssh -o ConnectTimeout=10 -o ServerAliveInterval=30 "$SECONDARY" \
      "$MONGOSH --quiet $DB --file /dev/stdin" > "$ttl_file"

    local t1=$(date +%s)
    local bytes=$(wc -c < "$ttl_file" | tr -d ' ')
    log "  Exported: $(( bytes / 1024 / 1024 )) MB in $(( t1 - t0 ))s"

    upload_ttl "$graph_uri" "$ttl_file" PUT
    rm -f "$ttl_file"
  else
    # Chunked upload
    local offset=0 chunk_num=0
    local total_chunks=$(( (total + CHUNK_LIMIT - 1) / CHUNK_LIMIT ))
    log "  Chunked: $total_chunks chunks of $CHUNK_LIMIT"

    while [ "$offset" -lt "$total" ]; do
      chunk_num=$((chunk_num + 1))
      local ttl_file="$HARVEST_DIR/${vol}_chunk${chunk_num}.ttl"
      log "  Chunk $chunk_num/$total_chunks (offset $offset)..."
      local t0=$(date +%s)

      printf "var VOLUME = '%s'; var OFFSET = %d; var CHUNK = %d;\n" \
        "$vol" "$offset" "$CHUNK_LIMIT" > "$HARVEST_DIR/_header.js"
      cat "$HARVEST_DIR/_header.js" "$SCRIPT_DIR/harvest-media-export.js" | \
        ssh -o ConnectTimeout=10 -o ServerAliveInterval=30 "$SECONDARY" \
        "$MONGOSH --quiet $DB --file /dev/stdin" > "$ttl_file"

      local t1=$(date +%s)
      local bytes=$(wc -c < "$ttl_file" | tr -d ' ')
      log "    Exported: $(( bytes / 1024 / 1024 )) MB in $(( t1 - t0 ))s"

      # First chunk: PUT (create). Rest: POST (append).
      if [ "$chunk_num" -eq 1 ]; then
        upload_ttl "$graph_uri" "$ttl_file" PUT
      else
        upload_ttl "$graph_uri" "$ttl_file" POST
      fi
      rm -f "$ttl_file"
      offset=$((offset + CHUNK_LIMIT))
    done
  fi

  local vol_seconds=$(( $(date +%s) - vol_start ))
  chorus_event harvest.pipeline.completed silas "collection=$vol" "items=$total" "type=media" "duration_seconds=$vol_seconds"
  notify_harvest "media/$vol" "completed" "$total" "${vol_seconds}s"
  _current_domain=""
  log "  Done: $vol (${vol_seconds}s)"
  echo ""
}

harvest_models() {
  local ttl_file="$HARVEST_DIR/models.ttl"
  local graph_uri="$GRAPH_BASE/models"

  log "=== Models ==="
  _current_domain="media/models"
  chorus_event harvest.pipeline.started silas "collection=models" "type=media"

  local t0=$(date +%s)
  ssh -o ConnectTimeout=10 -o ServerAliveInterval=30 "$SECONDARY" \
    "$MONGOSH --quiet $DB --file /dev/stdin" <<'EOF' > "$ttl_file"
print('@prefix jb: <https://jeffbridwell.com/ontology#> .');
print('@prefix dc: <http://purl.org/dc/terms/> .');
print('@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .');
print('@prefix foaf: <http://xmlns.com/foaf/0.1/> .');
print('');

const BASE = 'https://jeffbridwell.com/pods/jeff/media/models/';

function esc(s) {
  if (!s) return '';
  return String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '');
}

let count = 0;
db.models.find({}).sort({_id: 1}).forEach(doc => {
  const id = doc.checksum || doc._id.toString();
  const name = doc.filename ? doc.filename.replace(/\.[^.]+$/, '') : '';

  let props = [];
  props.push('    a jb:Model');
  if (name) props.push('    jb:modelName "' + esc(name) + '"');
  if (doc.checksum) props.push('    jb:modelChecksum "' + esc(doc.checksum) + '"');
  if (doc.path) props.push('    jb:filePath "' + esc(doc.path) + '"');
  if (doc.filename) props.push('    jb:photoFilename "' + esc(doc.filename) + '"');
  if (doc.base_attributes && doc.base_attributes.size) {
    props.push('    jb:fileSize ' + doc.base_attributes.size);
  }

  print('<' + BASE + encodeURIComponent(id) + '>');
  print(props.join(' ;\n') + ' .\n');
  count++;
});
print('# Total: ' + count + ' models');
EOF

  local t1=$(date +%s)
  local bytes=$(wc -c < "$ttl_file" | tr -d ' ')
  log "  Exported: $(( bytes / 1024 / 1024 )) MB, $(( t1 - t0 ))s"

  local model_count=$( grep -c '^<' "$ttl_file" 2>/dev/null || echo 0 )

  delete_graph "$graph_uri"
  upload_ttl "$graph_uri" "$ttl_file" PUT
  rm -f "$ttl_file"

  local model_seconds=$(( $(date +%s) - t0 ))
  chorus_event harvest.pipeline.completed silas "collection=models" "items=$model_count" "type=media" "duration_seconds=$model_seconds"
  notify_harvest "media/models" "completed" "$model_count" "${model_seconds}s"
  _current_domain=""
  log "  Done: models ($model_count items, ${model_seconds}s)"
  echo ""
}

cmd_verify() {
  log "Media graphs in Fuseki:"
  local auth_flag=""
  [ -n "$FUSEKI_PW" ] && auth_flag="-u admin:$FUSEKI_PW"

  curl -s "$FUSEKI_URL/$DATASET/query" \
    -H "Accept: text/csv" \
    --data-urlencode "query=SELECT ?g (COUNT(*) as ?triples) WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), '${GRAPH_BASE}')) } GROUP BY ?g ORDER BY DESC(?triples)" \
    $auth_flag 2>/dev/null | column -t -s,
}

cmd_harvest() {
  local target="${1:?Usage: $0 harvest <volume|all>}"

  if [ "$target" = "all" ]; then
    log "Full harvest — models + all volumes"
    local t_start=$(date +%s)

    harvest_models

    # Smallest first (WSJF)
    local volumes
    volumes=$(cmd_list 2>/dev/null | grep -v '^[0-9]' | grep -v '^---' | grep -v '^models' | sort -t$'\t' -k2 -n | cut -f1)
    for v in $volumes; do
      harvest_volume "$v"
    done

    local t_end=$(date +%s)
    log "Full harvest complete in $(( t_end - t_start ))s"
  else
    harvest_volume "$target"
  fi
}

# --- Main ---

_current_domain=""  # clear trap before dispatch
case "${1:-help}" in
  list)    cmd_list ;;
  harvest) shift; cmd_harvest "$@" ;;
  models)  harvest_models ;;
  verify)  cmd_verify ;;
  *)
    echo "harvest-media.sh — MongoDB → Fuseki RDF (volume-sharded collection graphs)"
    echo ""
    echo "Commands:"
    echo "  list              Show volumes and item counts"
    echo "  harvest <volume>  Harvest a single volume"
    echo "  harvest all       Harvest all volumes + models"
    echo "  models            Harvest models collection only"
    echo "  verify            Check media graphs in Fuseki"
    ;;
esac
