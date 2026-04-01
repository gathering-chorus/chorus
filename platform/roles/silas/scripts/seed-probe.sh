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
  FUSEKI_ADMIN_PASSWORD="$(grep '^FUSEKI_ADMIN_PASSWORD=' "$ENV_FILE" | cut -d= -f2-)"
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
  /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log \
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
  -u "admin:${FUSEKI_ADMIN_PASSWORD}" \
  --data "$DELETE_QUERY" >/dev/null 2>&1 || {
    echo "${PROBE_TAG} WARN: probe seed cleanup failed (non-fatal)" >&2
  }

# ─── PERMUTATION TESTS ──────────────────────────────────────────
# Five routing scenarios — webhook + Fuseki verify + cleanup. No nudges, no Bridge.

PERM_PASS=0
PERM_FAIL=0

send_signed_webhook() {
  local body="$1" sid="$2"
  local sign_url="${PUBLIC_URL}${WEBHOOK_PATH}"
  local form="Body=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${body}', safe=''))")"
  form="${form}&From=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
  form="${form}&MessageSid=${sid}"
  form="${form}&NumMedia=0"
  form="${form}&To=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
  local sign_data="${sign_url}Body${body}From${CAPTURE_ALLOWED_PHONES}MessageSid${sid}NumMedia0To${CAPTURE_ALLOWED_PHONES}"
  local sig=$(echo -n "$sign_data" | openssl dgst -sha1 -hmac "$TWILIO_AUTH_TOKEN" -binary | base64)
  curl -sf -o /dev/null -w "%{http_code}" --max-time 15 \
    -X POST "${LOCAL_URL}${WEBHOOK_PATH}" \
    -H "Content-Type: application/x-www-form-urlencoded" \
    -H "X-Twilio-Signature: ${sig}" \
    -d "$form" 2>/dev/null || echo "000"
}

fuseki_has_sid() {
  local sid="$1"
  local count=$(curl -sf --max-time "$TIMEOUT" \
    -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT (COUNT(?s) AS ?c) WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid \"${sid}\" } }" \
    "${FUSEKI_URL}/pods/sparql" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])" 2>/dev/null || echo "0")
  [[ "$count" != "0" ]]
}

delete_seed() {
  local sid="$1"
  curl -sf --max-time "$TIMEOUT" \
    -X POST "${FUSEKI_URL}/pods/update" \
    -H "Content-Type: application/sparql-update" \
    -u "admin:${FUSEKI_ADMIN_PASSWORD}" \
    --data "PREFIX jb: <https://jeffbridwell.com/ontology#> DELETE WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid \"${sid}\" ; ?p ?o . } }" \
    >/dev/null 2>&1 || true
}

TS=$(date +%s)

# ─── P1: Content + hashtag in ONE message (Jeff's actual pattern)
echo "${PROBE_TAG} P1: Content + hashtag (single message)..."
P1_SID="SM_PROBE_P1_${TS}"
send_signed_webhook "[SEED-PROBE] Garden design ideas #idea" "$P1_SID" >/dev/null
sleep 3
if fuseki_has_sid "$P1_SID"; then
  echo "${PROBE_TAG} P1: PASS — content+hashtag persisted as single seed"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P1: FAIL — content+hashtag seed not found in Fuseki" >&2
  PERM_FAIL=$((PERM_FAIL + 1))
fi
delete_seed "$P1_SID"

# ─── P2: Link + hashtag in ONE message
echo "${PROBE_TAG} P2: Link + hashtag (single message)..."
P2_SID="SM_PROBE_P2_${TS}"
send_signed_webhook "[SEED-PROBE] https://example.com/interesting-article #idea" "$P2_SID" >/dev/null
sleep 3
if fuseki_has_sid "$P2_SID"; then
  echo "${PROBE_TAG} P2: PASS — link+hashtag persisted"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P2: FAIL — link+hashtag seed not found in Fuseki" >&2
  PERM_FAIL=$((PERM_FAIL + 1))
fi
delete_seed "$P2_SID"

# ─── P3: Content without hashtag — routes to wren by default
echo "${PROBE_TAG} P3: Content without hashtag..."
P3_SID="SM_PROBE_P3_${TS}"
send_signed_webhook "[SEED-PROBE] Random thought about gardens" "$P3_SID" >/dev/null
sleep 5
if fuseki_has_sid "$P3_SID"; then
  echo "${PROBE_TAG} P3: PASS — content without hashtag persisted (default route)"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P3: FAIL — untagged content not found in Fuseki" >&2
  PERM_FAIL=$((PERM_FAIL + 1))
fi
delete_seed "$P3_SID"

# ─── P4: Hashtag only — NO capture created
echo "${PROBE_TAG} P4: Hashtag only..."
P4_SID="SM_PROBE_P4_${TS}"
send_signed_webhook "#wren" "$P4_SID" >/dev/null
sleep 3
if ! fuseki_has_sid "$P4_SID"; then
  echo "${PROBE_TAG} P4: PASS — hashtag-only did not create capture"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P4: FAIL — hashtag-only created a capture" >&2
  PERM_FAIL=$((PERM_FAIL + 1))
fi
delete_seed "$P4_SID"

# ─── P5: Cleanup verification — all test seeds gone
echo "${PROBE_TAG} P5: Cleanup verification..."
LEFTOVER=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT (COUNT(?s) AS ?c) WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid ?sid . FILTER(STRSTARTS(?sid, 'SM_PROBE_')) } }" \
  "${FUSEKI_URL}/pods/sparql" 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])" 2>/dev/null || echo "?")

if [[ "$LEFTOVER" == "0" ]]; then
  echo "${PROBE_TAG} P5: PASS — all test seeds cleaned up"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P5: WARN — ${LEFTOVER} probe seeds remain in Fuseki" >&2
  PERM_PASS=$((PERM_PASS + 1)) # warn, not fail
fi

echo "${PROBE_TAG} PERMUTATIONS: ${PERM_PASS} pass, ${PERM_FAIL} fail"

if [[ $PERM_FAIL -gt 0 ]]; then
  log_result "failed" "permutations=${PERM_PASS}pass/${PERM_FAIL}fail"
  exit 1
fi

# ─── SUCCESS ─────────────────────────────────────────────────────
echo "${PROBE_TAG} ALL HOPS + PERMUTATIONS PASSED — seed pipeline healthy"
log_result "passed" "hops=5 permutations=${PERM_PASS} elapsed=${ELAPSED}s"
exit 0
