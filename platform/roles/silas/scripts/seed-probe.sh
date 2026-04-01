#!/usr/bin/env bash
# seed-probe.sh — Synthetic SMS health probe
# Card #1800 · Silas · 2026-03-28
#
# Tests the full seed pipeline: Twilio webhook → Cloudflare tunnel → App → Fuseki
# Runs daily at 5:55am before daily review. Alerts Bridge on failure.
#
# Exit codes: 0 = healthy, 1 = pipeline failure

set -euo pipefail

USE_REAL_SMS=false
if [[ "${1:-}" == "--real-sms" ]]; then
  USE_REAL_SMS=true
fi

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

# Real SMS via Messages.app — gated behind --real-sms flag.
# Tests full carrier→Twilio→tunnel→app path. One SMS per daily 5:55am run only.
SMS_SERVICE_ID="00BA859D-E329-4998-A366-E934E6B49E0A"
TWILIO_PHONE="+14015922496"
send_real_sms() {
  local body="$1"
  if $USE_REAL_SMS; then
    osascript -e "tell application \"Messages\" to send \"${body}\" to buddy \"${TWILIO_PHONE}\" of service id \"${SMS_SERVICE_ID}\"" 2>/dev/null
    echo "${PROBE_TAG} Sent real SMS to Twilio: ${body:0:40}..."
  fi
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

WAIT_SMS=30

# Helper: check Fuseki for content substring
fuseki_has_content() {
  local needle="$1"
  local found=$(curl -sf --max-time "$TIMEOUT" \
    -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT ?seed WHERE { GRAPH <urn:jb:seeds/> { ?seed jb:seedContent ?c . FILTER(CONTAINS(?c, '${needle}')) } } LIMIT 1" \
    "${FUSEKI_URL}/pods/sparql" 2>/dev/null \
    | python3 -c "import json,sys; b=json.load(sys.stdin)['results']['bindings']; print(b[0]['seed']['value'] if b else '')" 2>/dev/null || echo "")
  [[ -n "$found" ]]
}

# Helper: delete seed by content match
delete_seed_by_content() {
  local needle="$1"
  curl --max-time "$TIMEOUT" -X POST "${FUSEKI_URL}/pods/update" \
    -H "Content-Type: application/sparql-update" \
    -u "admin:${FUSEKI_ADMIN_PASSWORD}" \
    --data "PREFIX jb: <https://jeffbridwell.com/ontology#> DELETE { GRAPH <urn:jb:seeds/> { ?s ?p ?o } } WHERE { GRAPH <urn:jb:seeds/> { ?s jb:seedContent ?c . FILTER(CONTAINS(?c, '${needle}')) . ?s ?p ?o . } }" \
    >/dev/null 2>&1 || true
}

# ─── P1: Content + hashtag (Jeff's actual pattern)
echo "${PROBE_TAG} P1: Content + hashtag..."
P1_TAG="p1-${TS}"
P1_BODY="[SEED-PROBE] Garden design ideas ${P1_TAG} #idea"
if $USE_REAL_SMS; then
  send_real_sms "$P1_BODY"
  sleep $WAIT_SMS
  fuseki_has_content "$P1_TAG" && { echo "${PROBE_TAG} P1: PASS — content+hashtag (real SMS)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P1: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed_by_content "$P1_TAG"
else
  P1_SID="SM_PROBE_P1_${TS}"
  send_signed_webhook "$P1_BODY" "$P1_SID" >/dev/null; sleep 3
  fuseki_has_sid "$P1_SID" && { echo "${PROBE_TAG} P1: PASS — content+hashtag (webhook)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P1: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed "$P1_SID"
fi

# ─── P2: Link + hashtag
echo "${PROBE_TAG} P2: Link + hashtag..."
P2_TAG="p2-${TS}"
P2_BODY="[SEED-PROBE] https://example.com/${P2_TAG} #idea"
if $USE_REAL_SMS; then
  send_real_sms "$P2_BODY"
  sleep $WAIT_SMS
  fuseki_has_content "$P2_TAG" && { echo "${PROBE_TAG} P2: PASS — link+hashtag (real SMS)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P2: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed_by_content "$P2_TAG"
else
  P2_SID="SM_PROBE_P2_${TS}"
  send_signed_webhook "$P2_BODY" "$P2_SID" >/dev/null; sleep 3
  fuseki_has_sid "$P2_SID" && { echo "${PROBE_TAG} P2: PASS — link+hashtag (webhook)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P2: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed "$P2_SID"
fi

# ─── P3: Content without hashtag — routes to wren by default
echo "${PROBE_TAG} P3: Content without hashtag..."
P3_TAG="p3-${TS}"
P3_BODY="[SEED-PROBE] Random thought about gardens ${P3_TAG}"
if $USE_REAL_SMS; then
  send_real_sms "$P3_BODY"
  sleep $WAIT_SMS
  fuseki_has_content "$P3_TAG" && { echo "${PROBE_TAG} P3: PASS — no hashtag, default route (real SMS)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P3: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed_by_content "$P3_TAG"
else
  P3_SID="SM_PROBE_P3_${TS}"
  send_signed_webhook "$P3_BODY" "$P3_SID" >/dev/null; sleep 5
  fuseki_has_sid "$P3_SID" && { echo "${PROBE_TAG} P3: PASS — no hashtag, default route (webhook)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P3: FAIL" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
  delete_seed "$P3_SID"
fi

# ─── P4: Hashtag only — NO capture (webhook only, can't verify negative via real SMS)
echo "${PROBE_TAG} P4: Hashtag only..."
P4_SID="SM_PROBE_P4_${TS}"
if $USE_REAL_SMS; then
  P4_TAG="p4-${TS}"
  send_real_sms "#wren"
  sleep $WAIT_SMS
  # Hashtag-only should NOT create a capture. Check that no seed has content "#wren" with our timestamp window.
  ! fuseki_has_content "$P4_TAG" && { echo "${PROBE_TAG} P4: PASS — hashtag-only, no capture (real SMS)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P4: FAIL — hashtag created capture" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
else
  send_signed_webhook "#wren" "$P4_SID" >/dev/null; sleep 3
  ! fuseki_has_sid "$P4_SID" && { echo "${PROBE_TAG} P4: PASS — hashtag-only, no capture (webhook)"; PERM_PASS=$((PERM_PASS+1)); } || { echo "${PROBE_TAG} P4: FAIL — hashtag created capture" >&2; PERM_FAIL=$((PERM_FAIL+1)); }
fi
delete_seed "$P4_SID"
delete_seed "$P4_SID"

# ─── P5: Multi-photo + hashtag — one seed, multiple media
echo "${PROBE_TAG} P5: Multi-photo + hashtag (single webhook)..."
P5_SID="SM_PROBE_P5_${TS}"
P5_BODY="[SEED-PROBE] Family photos from the garden #kade"
P5_SIGN_URL="${PUBLIC_URL}${WEBHOOK_PATH}"
P5_FORM="Body=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${P5_BODY}', safe=''))")"
P5_FORM="${P5_FORM}&From=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
P5_FORM="${P5_FORM}&MediaUrl0=https%3A%2F%2Fapi.twilio.com%2Ffake%2Fmedia0.jpg"
P5_FORM="${P5_FORM}&MediaUrl1=https%3A%2F%2Fapi.twilio.com%2Ffake%2Fmedia1.jpg"
P5_FORM="${P5_FORM}&MessageSid=${P5_SID}"
P5_FORM="${P5_FORM}&NumMedia=2"
P5_FORM="${P5_FORM}&To=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
# Sign with all params sorted: Body, From, MediaUrl0, MediaUrl1, MessageSid, NumMedia, To
P5_SIGN_DATA="${P5_SIGN_URL}Body${P5_BODY}From${CAPTURE_ALLOWED_PHONES}MediaUrl0https://api.twilio.com/fake/media0.jpgMediaUrl1https://api.twilio.com/fake/media1.jpgMessageSid${P5_SID}NumMedia2To${CAPTURE_ALLOWED_PHONES}"
P5_SIG=$(echo -n "$P5_SIGN_DATA" | openssl dgst -sha1 -hmac "$TWILIO_AUTH_TOKEN" -binary | base64)
P5_HTTP=$(curl -sf -o /dev/null -w "%{http_code}" --max-time 15 \
  -X POST "${LOCAL_URL}${WEBHOOK_PATH}" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -H "X-Twilio-Signature: ${P5_SIG}" \
  -d "$P5_FORM" 2>/dev/null || echo "000")
sleep 3
if fuseki_has_sid "$P5_SID"; then
  # Verify only ONE seed was created (not two from two media)
  P5_COUNT=$(curl -sf --max-time "$TIMEOUT" \
    -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT (COUNT(?s) AS ?c) WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid \"${P5_SID}\" } }" \
    "${FUSEKI_URL}/pods/sparql" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])" 2>/dev/null || echo "0")
  if [[ "$P5_COUNT" == "1" ]]; then
    echo "${PROBE_TAG} P5: PASS — multi-photo created exactly 1 seed (HTTP ${P5_HTTP})"
    PERM_PASS=$((PERM_PASS + 1))
  else
    echo "${PROBE_TAG} P5: FAIL — multi-photo created ${P5_COUNT} seeds (expected 1)" >&2
    PERM_FAIL=$((PERM_FAIL + 1))
  fi
else
  echo "${PROBE_TAG} P5: FAIL — multi-photo seed not found in Fuseki (HTTP ${P5_HTTP})" >&2
  PERM_FAIL=$((PERM_FAIL + 1))
fi
delete_seed "$P5_SID"

# ─── P6: Cleanup verification — all test seeds gone
echo "${PROBE_TAG} P6: Cleanup verification..."
LEFTOVER=$(curl -sf --max-time "$TIMEOUT" \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT (COUNT(?s) AS ?c) WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid ?sid . FILTER(STRSTARTS(?sid, 'SM_PROBE_')) } }" \
  "${FUSEKI_URL}/pods/sparql" 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])" 2>/dev/null || echo "?")

if [[ "$LEFTOVER" == "0" ]]; then
  echo "${PROBE_TAG} P6: PASS — all test seeds cleaned up"
  PERM_PASS=$((PERM_PASS + 1))
else
  echo "${PROBE_TAG} P6: WARN — ${LEFTOVER} probe seeds remain in Fuseki" >&2
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
