#!/usr/bin/env bats
# @test-type: contract — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
# @domain: search — the product domain this suite guards (#4334)
load test_helper
# api-fragile-endpoints.bats — E2E tests for fragile API endpoints (#1776)
# What Jeff sees: endpoints that break silently — Athena 500s, completeness timeouts,
# seed webhook drops. These tests run against the real stack, no mocks.
# Contract-level assertions only: URL + response shape.

CHORUS_API="http://localhost:3340"
APP_API="http://localhost:3000"

setup() {
  # #4336: a live-only contract suite. When chorus-api does not answer, the
  # questions below are UNMEASURED, never red and never green.
  curl -s -o /dev/null --max-time 3 http://localhost:3340/api/chorus/health \
    || skip "UNMEASURED — chorus-api :3340 not answering (#4336)"
}


# --- AC 4: Seed webhook returns 200 with valid Twilio payload ---

@test "POST /api/seed/sms returns 200 with signed Twilio payload" {
  # #4336: this posts a signed SMS to the live seed webhook, which files a real
  # seed, and it reads the app's .env for the Twilio secret. Only a run that
  # means to do both may run it.
  [ "${RUN_LIVE_INTEGRATION:-}" = "true" ] || skip "UNMEASURED — writes a real seed and reads app secrets; RUN_LIVE_INTEGRATION=true to run (#4336)"
  # Load credentials
  ENV_FILE="${HOME}/CascadeProjects/jeff-bridwell-personal-site/.env"
  [ -f "$ENV_FILE" ] || skip "No .env file"
  TWILIO_AUTH_TOKEN=$(grep '^TWILIO_AUTH_TOKEN=' "$ENV_FILE" | cut -d= -f2-)
  CAPTURE_ALLOWED_PHONES=$(grep '^CAPTURE_ALLOWED_PHONES=' "$ENV_FILE" | cut -d= -f2- | cut -d, -f1)
  [ -n "$TWILIO_AUTH_TOKEN" ] || skip "No Twilio auth token"

  PROBE_SID="SM_PROBE_E2E_$(date +%s)"
  PROBE_BODY="[SEED-PROBE] E2E test $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  WEBHOOK_URL="${APP_API}/api/seed/sms"
  # Sign against public URL (Twilio signature spec) but send to localhost
  SIGN_URL="https://lightlifeurbangardens.com/api/seed/sms"

  # Build signed request
  SIGN_DATA="${SIGN_URL}Body${PROBE_BODY}From${CAPTURE_ALLOWED_PHONES}MessageSid${PROBE_SID}NumMedia0To${CAPTURE_ALLOWED_PHONES}"
  SIGNATURE=$(echo -n "$SIGN_DATA" | openssl dgst -sha1 -hmac "$TWILIO_AUTH_TOKEN" -binary | base64)

  BODY="Body=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${PROBE_BODY}', safe=''))")"
  BODY="${BODY}&From=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"
  BODY="${BODY}&MessageSid=${PROBE_SID}"
  BODY="${BODY}&NumMedia=0"
  BODY="${BODY}&To=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${CAPTURE_ALLOWED_PHONES}', safe=''))")"

  http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 \
    -X POST "$WEBHOOK_URL" \
    -H "Content-Type: application/x-www-form-urlencoded" \
    -H "X-Twilio-Signature: ${SIGNATURE}" \
    -d "$BODY" 2>/dev/null)
  [ "$http_code" = "200" ]
}

# --- AC 5: Chorus search returns results array ---

@test "GET /api/chorus/search returns results array" {
  result=$(curl -sf "$CHORUS_API/api/chorus/search?q=test&limit=3" 2>/dev/null)
  [ $? -eq 0 ]
  has_results=$(echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); print('yes' if isinstance(d.get('results'), list) else 'no')" 2>/dev/null)
  [ "$has_results" = "yes" ]
}

@test "GET /api/chorus/search with empty query returns 400" {
  http_code=$(curl -s -o /dev/null -w "%{http_code}" "$CHORUS_API/api/chorus/search?q=&limit=3" 2>/dev/null)
  [ "$http_code" = "400" ]
}
