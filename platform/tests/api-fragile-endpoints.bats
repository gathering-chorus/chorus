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

# --- Athena subdomain tests: DELETED by #4237, 2026-09-21 ---
#
# Four tests lived here asserting /api/athena/subdomains served a non-empty list,
# a detail document for chorus-domain, a 404 for an unknown id, and completeness
# inside five seconds. All four went red in the nightly, and Kade triaged them to
# this card rather than fixing them in his own.
#
# They are not fixed, they are retired: chorus:SubDomain is gone. Jeff ruled retire
# on 2026-06-19 (#3509) and this card carried it out — 49 rows retyped chorus:Domain
# and the class deleted from the model. A test asserting a retired class still
# serves is a test that would have to be un-fixed later.
#
# What replaces them: the generated quartet in designing/products/*/domains/*/
# tests.json, executed by platform/tests/4237-generated-api-quartet.test.sh. It
# covers the same endpoints for every class the model declares, including Domain,
# and asserts the contract the shape defines rather than a row count.


# --- AC 4: Seed webhook returns 200 with valid Twilio payload ---

@test "POST /api/seed/sms returns 200 with signed Twilio payload" {
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
