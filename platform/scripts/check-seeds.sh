#!/usr/bin/env bash
# check-seeds.sh — Deterministic seed pipeline status
# Card #1995 · Kade · 2026-04-03
#
# Any role runs this. No interpretation needed. Output is the answer.

set -euo pipefail

FUSEKI_URL="http://localhost:3030"
APP_URL="http://localhost:3000"
TUNNEL_URL="https://lightlifeurbangardens.com"
TIMEOUT=5

# ─── Pipeline health ───────────────────────────────────────────
tunnel=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "${TUNNEL_URL}/health" 2>/dev/null || echo "000")
app=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "${APP_URL}/health" 2>/dev/null || echo "000")
fuseki=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "${FUSEKI_URL}/\$/ping" 2>/dev/null || echo "000")

# ─── Fuseki write probe — canary INSERT + DELETE ──────────────
fuseki_write="OK"
if [[ "$fuseki" == "200" ]]; then
  CANARY="urn:jb:seeds/_canary_$(date +%s)"
  FUSEKI_PW=$(sed -n 's/^FUSEKI_ADMIN_[^=]*=//p' /Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env 2>/dev/null | head -1)
  INSERT_OK=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
    -X POST "${FUSEKI_URL}/pods/update" \
    -H "Content-Type: application/sparql-update" \
    -u "admin:${FUSEKI_PW}" \
    --data "INSERT DATA { GRAPH <urn:jb:seeds/> { <${CANARY}> <urn:probe> \"canary\" } }" 2>/dev/null || echo "000")
  DELETE_OK=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
    -X POST "${FUSEKI_URL}/pods/update" \
    -H "Content-Type: application/sparql-update" \
    -u "admin:${FUSEKI_PW}" \
    --data "DELETE DATA { GRAPH <urn:jb:seeds/> { <${CANARY}> <urn:probe> \"canary\" } }" 2>/dev/null || echo "000")
  if [[ "$INSERT_OK" != "200" && "$INSERT_OK" != "204" ]] || [[ "$DELETE_OK" != "200" && "$DELETE_OK" != "204" ]]; then
    fuseki_write="FAIL"
  fi
fi

if [[ "$tunnel" == "200" && "$app" == "200" && "$fuseki" == "200" && "$fuseki_write" == "OK" ]]; then
  PIPELINE="healthy"
elif [[ "$fuseki" == "200" && "$fuseki_write" == "FAIL" ]]; then
  PIPELINE="DOWN (fuseki: reads OK, writes FAIL)"
else
  PIPELINE="DOWN"
fi

# ─── Pending seeds (SPARQL) ────────────────────────────────────
PENDING_QUERY='PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?seed ?content ?type ?created ?hashtag WHERE {
  GRAPH <urn:jb:seeds/> {
    ?seed jb:hasSeedStatus ?status .
    FILTER(?status NOT IN (jb:Routed, jb:Discarded))
    ?seed jb:seedContent ?content .
    ?seed jb:hasSeedType ?type .
    OPTIONAL { ?seed jb:seededAt ?created }
    OPTIONAL { ?seed jb:seedHashtag ?hashtag }
  }
} ORDER BY DESC(?created)'

PENDING_JSON=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=${PENDING_QUERY}" \
  "${FUSEKI_URL}/pods/query" 2>/dev/null || echo '{"results":{"bindings":[]}}')

PENDING_COUNT=$(echo "$PENDING_JSON" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['results']['bindings']))" 2>/dev/null || echo "?")

# ─── Last seed timestamp ──────────────────────────────────────
LAST_QUERY='PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?created WHERE {
  GRAPH <urn:jb:seeds/> {
    ?seed jb:seededAt ?created .
  }
} ORDER BY DESC(?created) LIMIT 1'

LAST_SEED=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=${LAST_QUERY}" \
  "${FUSEKI_URL}/pods/query" 2>/dev/null \
  | python3 -c "
import sys,json
from datetime import datetime, timezone, timedelta
b=json.load(sys.stdin)['results']['bindings']
if b:
    ts=b[0]['created']['value']
    dt=datetime.fromisoformat(ts.replace('Z','+00:00'))
    boston=dt.astimezone(timezone(timedelta(hours=-4)))
    print(boston.strftime('%Y-%m-%d %H:%M Boston'))
else:
    print('unknown')
" 2>/dev/null || echo "unknown")

# ─── Total seed count ─────────────────────────────────────────
TOTAL_QUERY='PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT (COUNT(?seed) AS ?c) WHERE {
  GRAPH <urn:jb:seeds/> { ?seed a ?type }
}'

TOTAL=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=${TOTAL_QUERY}" \
  "${FUSEKI_URL}/pods/query" 2>/dev/null \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])" 2>/dev/null || echo "?")

# ─── Last 3 received seeds ────────────────────────────────────
RECENT_QUERY='PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?content ?type ?status ?created ?hashtag ?media WHERE {
  GRAPH <urn:jb:seeds/> {
    ?seed jb:seedContent ?content .
    ?seed jb:hasSeedType ?type .
    ?seed jb:hasSeedStatus ?status .
    OPTIONAL { ?seed jb:seededAt ?created }
    OPTIONAL { ?seed jb:seedHashtag ?hashtag }
    OPTIONAL { ?seed jb:seedMediaPath ?media }
  }
} ORDER BY DESC(?created) LIMIT 3'

RECENT_JSON=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=${RECENT_QUERY}" \
  "${FUSEKI_URL}/pods/query" 2>/dev/null || echo '{"results":{"bindings":[]}}')

# ─── Output ───────────────────────────────────────────────────
echo ""
echo "Seeds: ${PENDING_COUNT} pending (${TOTAL} total)"
echo "Pipeline: ${PIPELINE} (tunnel=${tunnel} app=${app} fuseki=${fuseki})"
echo "Last seed: ${LAST_SEED}"

echo ""
echo "Recent:"
echo "$RECENT_JSON" | python3 -c "
import sys,json
from datetime import datetime, timezone, timedelta
d=json.load(sys.stdin)
for b in d['results']['bindings']:
    content=b.get('content',{}).get('value','')[:50]
    status=b.get('status',{}).get('value','').split('#')[-1]
    stype=b.get('type',{}).get('value','').split('#')[-1]
    hashtag=b.get('hashtag',{}).get('value','')
    media=b.get('media',{}).get('value','')
    ts=b.get('created',{}).get('value','')
    if ts:
        try:
            dt=datetime.fromisoformat(ts.replace('Z','+00:00'))
            boston=dt.astimezone(timezone(timedelta(hours=-4)))
            ts=boston.strftime('%m/%d %H:%M')
        except: pass
    # Show type prefix for media seeds where content is just a hashtag
    display = content
    if media and content.startswith('#'):
        label = {'PhotoCapture': 'Photo', 'AudioCapture': 'Audio', 'VideoCapture': 'Video'}.get(stype, stype)
        display = f'{label} ({content})'
    tag = f' {hashtag}' if hashtag else ''
    print(f'  {ts}  [{status}]  {display}{tag}')
" 2>/dev/null

if [[ "$PIPELINE" == "DOWN" ]]; then
  echo ""
  echo "!! Seeds sent now will not be captured. Fix pipeline first."
fi

# ─── Pending seed details ─────────────────────────────────────
if [[ "$PENDING_COUNT" != "0" && "$PENDING_COUNT" != "?" ]]; then
  echo ""
  echo "$PENDING_JSON" | python3 -c "
import sys,json
from datetime import datetime, timezone, timedelta
d=json.load(sys.stdin)
bindings=d['results']['bindings']
print(f'  # | Type       | Content                                  | Hashtag  | Received')
print(f'  --+------------+------------------------------------------+----------+-------------------')
for i,b in enumerate(bindings):
    stype=b.get('type',{}).get('value','').split('#')[-1][:10]
    content=b.get('content',{}).get('value','')[:40]
    hashtag=b.get('hashtag',{}).get('value','')[:8]
    ts=b.get('created',{}).get('value','')
    if ts:
        try:
            dt=datetime.fromisoformat(ts.replace('Z','+00:00'))
            boston=dt.astimezone(timezone(timedelta(hours=-4)))
            ts=boston.strftime('%Y-%m-%d %H:%M')
        except: pass
    print(f'  {i+1:>1} | {stype:<10} | {content:<40} | {hashtag:<8} | {ts}')
" 2>/dev/null
fi
echo ""
