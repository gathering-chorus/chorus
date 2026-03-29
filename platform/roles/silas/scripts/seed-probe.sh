#!/usr/bin/env bash
# seed-probe.sh — Synthetic SMS health probe
# Card #1800 · Silas · 2026-03-28
#
# Tests the full seed pipeline: Twilio webhook → Cloudflare tunnel → App → Fuseki
# Runs daily at 5:55am before daily review. Alerts Bridge on failure.
#
# Exit codes: 0 = healthy, 1 = pipeline failure

set -euo pipefail

PROBE_TAG="[SEED-PROBE]"
PROBE_SID="SM_PROBE_$(date +%s)"
PROBE_CONTENT="${PROBE_TAG} synthetic health check $(date -u +%Y-%m-%dT%H:%M:%SZ)"
PUBLIC_URL="https://lightlifeurbangardens.com"
LOCAL_URL="http://localhost:3000"
FUSEKI_URL="http://localhost:3030"
BRIDGE_URL="http://localhost:3470/api/message"
WEBHOOK_PATH="/api/seed/sms"
TIMEOUT=5
POLL_TIMEOUT=30
POLL_INTERVAL=3

# Load Twilio credentials from app .env
ENV_FILE="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env"
if [[ -f "$ENV_FILE" ]]; then
  TWILIO_AUTH_TOKEN="$(grep '^TWILIO_AUTH_TOKEN=' "$ENV_FILE" | cut -d= -f2-)"
  CAPTURE_ALLOWED_PHONES="$(grep '^CAPTURE_ALLOWED_PHONES=' "$ENV_FILE" | cut -d= -f2- | cut -d, -f1)"
else
  echo "${PROBE_TAG} FAIL: .env not found" >&2
  exit 1
fi

if [[ -z "${TWILIO_AUTH_TOKEN:-}" || -z "${CAPTURE_ALLOWED_PHONES:-}" ]]; then
  echo "${PROBE_TAG} FAIL: missing TWILIO_AUTH_TOKEN or CAPTURE_ALLOWED_PHONES" >&2
  exit 1
fi

# Diagnostic accumulator
DIAG=""
FAILED_HOP=""

alert_bridge() {
  local msg="$1"
  curl -sf -X POST "$BRIDGE_URL" \
    -H "Content-Type: application/json" \
    -d "{\"role\":\"silas\",\"text\":\"${PROBE_TAG} ${msg}\"}" \
    >/dev/null 2>&1 || true
}

log_result() {
  local status="$1" detail="$2"
  bash /Users/jeffbridwell/CascadeProjects/messages/scripts/chorus-log.sh \
    seed.probe.${status} silas "detail=${detail}" 2>/dev/null || true
}

# ─── HOP 1: Cloudflare tunnel ───────────────────────────────────
echo "${PROBE_TAG} Hop 1: Cloudflare tunnel..."
TUNNEL_HTTP=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
  "${PUBLIC_URL}/health" 2>/dev/null || echo "000")

if [[ "$TUNNEL_HTTP" != "200" ]]; then
  FAILED_HOP="tunnel"
  DIAG="Cloudflare tunnel unreachable (HTTP ${TUNNEL_HTTP}). External SMS cannot reach the app."
  echo "${PROBE_TAG} FAIL: ${DIAG}" >&2
  alert_bridge "FAIL hop=tunnel — ${DIAG}"
  log_result "failed" "hop=tunnel http=${TUNNEL_HTTP}"
  exit 1
fi
echo "${PROBE_TAG} Hop 1: OK (tunnel alive)"

# ─── HOP 2: App health ──────────────────────────────────────────
echo "${PROBE_TAG} Hop 2: App health..."
APP_HTTP=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
  "${LOCAL_URL}/health" 2>/dev/null || echo "000")

if [[ "$APP_HTTP" != "200" ]]; then
  FAILED_HOP="app"
  DIAG="App not responding (HTTP ${APP_HTTP}). Webhook handler unreachable."
  echo "${PROBE_TAG} FAIL: ${DIAG}" >&2
  alert_bridge "FAIL hop=app — ${DIAG}"
  log_result "failed" "hop=app http=${APP_HTTP}"
  exit 1
fi
echo "${PROBE_TAG} Hop 2: OK (app healthy)"

# ─── HOP 3: Fuseki health ───────────────────────────────────────
echo "${PROBE_TAG} Hop 3: Fuseki health..."
FUSEKI_HTTP=$(curl -sf -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
  "${FUSEKI_URL}/\$/ping" 2>/dev/null || echo "000")

if [[ "$FUSEKI_HTTP" != "200" ]]; then
  FAILED_HOP="fuseki"
  DIAG="Fuseki not responding (HTTP ${FUSEKI_HTTP}). Seed persistence would fail."
  echo "${PROBE_TAG} FAIL: ${DIAG}" >&2
  alert_bridge "FAIL hop=fuseki — ${DIAG}"
  log_result "failed" "hop=fuseki http=${FUSEKI_HTTP}"
  exit 1
fi
echo "${PROBE_TAG} Hop 3: OK (Fuseki alive)"

# ─── HOP 4: Webhook delivery (signed, through tunnel) ───────────
echo "${PROBE_TAG} Hop 4: Sending synthetic SMS through tunnel..."

# Build form-encoded body (Twilio sends application/x-www-form-urlencoded)
WEBHOOK_URL="${PUBLIC_URL}${WEBHOOK_PATH}"

# Body params sorted alphabetically (Twilio signature spec)
BODY="Body=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${PROBE_CONTENT}', safe=''))")"
BODY="${BODY}&From=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
BODY="${BODY}&MessageSid=${PROBE_SID}"
BODY="${BODY}&NumMedia=0"
BODY="${BODY}&To=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"

# Compute Twilio signature: HMAC-SHA1(auth_token, url + sorted_params)
# Twilio spec: signature = Base64(HMAC-SHA1(AuthToken, URL + sorted param key=value pairs))
SIGN_DATA="${WEBHOOK_URL}Body${PROBE_CONTENT}From${CAPTURE_ALLOWED_PHONES}MessageSid${PROBE_SID}NumMedia0To${CAPTURE_ALLOWED_PHONES}"
SIGNATURE=$(echo -n "$SIGN_DATA" | openssl dgst -sha1 -hmac "$TWILIO_AUTH_TOKEN" -binary | base64)

WEBHOOK_HTTP=$(curl -sf -o /dev/null -w "%{http_code}" --max-time 15 \
  -X POST "$WEBHOOK_URL" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -H "X-Twilio-Signature: ${SIGNATURE}" \
  -d "$BODY" 2>/dev/null || echo "000")

if [[ "$WEBHOOK_HTTP" != "200" ]]; then
  FAILED_HOP="webhook"
  DIAG="Webhook returned HTTP ${WEBHOOK_HTTP}. Tunnel/app alive but handler rejected the request."
  echo "${PROBE_TAG} FAIL: ${DIAG}" >&2
  alert_bridge "FAIL hop=webhook — ${DIAG}"
  log_result "failed" "hop=webhook http=${WEBHOOK_HTTP}"
  exit 1
fi
echo "${PROBE_TAG} Hop 4: OK (webhook accepted)"

# ─── HOP 5: Verify seed landed in Fuseki ────────────────────────
echo "${PROBE_TAG} Hop 5: Polling Fuseki for probe seed (${POLL_TIMEOUT}s window)..."

SPARQL_QUERY="PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?seed WHERE {
  GRAPH <urn:jb:seeds/> {
    ?seed jb:messageSid \"${PROBE_SID}\" .
  }
} LIMIT 1"

SEED_FOUND=""
ELAPSED=0

while [[ $ELAPSED -lt $POLL_TIMEOUT ]]; do
  RESULT=$(curl -sf --max-time "$TIMEOUT" \
    -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=${SPARQL_QUERY}" \
    "${FUSEKI_URL}/pods/sparql" 2>/dev/null || echo "")

  if echo "$RESULT" | grep -q "seed"; then
    SEED_FOUND=$(echo "$RESULT" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r['results']['bindings'][0]['seed']['value'])" 2>/dev/null || echo "")
    if [[ -n "$SEED_FOUND" ]]; then
      echo "${PROBE_TAG} Hop 5: OK — seed found in Fuseki after ${ELAPSED}s"
      break
    fi
  fi

  sleep "$POLL_INTERVAL"
  ELAPSED=$((ELAPSED + POLL_INTERVAL))
done

if [[ -z "$SEED_FOUND" ]]; then
  FAILED_HOP="capture"
  DIAG="Webhook accepted (200) but seed not found in Fuseki within ${POLL_TIMEOUT}s. Handler may have failed silently."
  echo "${PROBE_TAG} FAIL: ${DIAG}" >&2
  alert_bridge "FAIL hop=capture — ${DIAG}"
  log_result "failed" "hop=capture sid=${PROBE_SID}"
  exit 1
fi

# ─── CLEANUP: Delete probe seed from Fuseki ──────────────────────
echo "${PROBE_TAG} Cleanup: removing probe seed..."

DELETE_QUERY="PREFIX jb: <https://jeffbridwell.com/ontology#>
DELETE WHERE {
  GRAPH <urn:jb:seeds/> {
    ?s jb:messageSid \"${PROBE_SID}\" ;
       ?p ?o .
  }
}"

curl -sf --max-time "$TIMEOUT" \
  -X POST "${FUSEKI_URL}/pods/update" \
  -H "Content-Type: application/sparql-update" \
  -u "admin:admin123" \
  --data "$DELETE_QUERY" >/dev/null 2>&1 || {
    echo "${PROBE_TAG} WARN: probe seed cleanup failed (non-fatal)" >&2
  }

# ─── SUCCESS ─────────────────────────────────────────────────────
echo "${PROBE_TAG} ALL HOPS PASSED — seed pipeline healthy"
log_result "passed" "all_hops=ok elapsed=${ELAPSED}s"
exit 0
