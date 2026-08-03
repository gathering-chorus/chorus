#!/usr/bin/env bats
# @test-type: integration — hits live chorus-api on :3340 (reads + authenticated writes)
# knowledge-domain.bats — Tests for knowledge domain (#1905)
# What Jeff sees: roles rediscover decisions from last week because the system
# doesn't surface relevant docs when entering a domain. After the fix:
# "3 artifacts govern this domain" appears at card pull.


# #3606 — doc-catalog moved to chorus-api. The app on :3000 now 301-redirects
# /api/doc-catalog to :3340 (DEC-093: all API endpoints live on the Chorus API),
# and these called it with `curl -sf` and no -L, so they got an empty body from
# the redirect and failed in the JSON parse. The suite already defined CHORUS_API
# and never used it. Endpoint was 200 and correct the whole time.
APP_API="http://localhost:3000"
CHORUS_API="http://localhost:3340"

# --- AC 3: Doc-catalog handler wired as domain's primary service ---

@test "GET /api/doc-catalog returns grouped docs" {
  result=$(curl -sf "$CHORUS_API/api/doc-catalog" 2>/dev/null)
  [ $? -eq 0 ]
  total=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin).get('totalDocs',0))" 2>/dev/null)
  [ "$total" -gt 0 ]
}

# --- AC 4: Domain endpoint shows artifacts per domain ---

@test "GET /api/doc-catalog/domain/:domain returns artifacts for a domain" {
  result=$(curl -sf "$CHORUS_API/api/doc-catalog/domain/seeds" 2>/dev/null)
  [ $? -eq 0 ]
  has_domain=$(echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); print('yes' if d.get('domain') else 'no')" 2>/dev/null)
  [ "$has_domain" = "yes" ]
}

@test "GET /api/doc-catalog/domain/:domain includes health metrics" {
  result=$(curl -sf "$CHORUS_API/api/doc-catalog/domain/chorus" 2>/dev/null)
  [ $? -eq 0 ]
  has_health=$(echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); print('yes' if 'health' in d else 'no')" 2>/dev/null)
  [ "$has_health" = "yes" ]
}

@test "GET /api/doc-catalog/domain/:domain returns governs and references arrays" {
  result=$(curl -sf "$CHORUS_API/api/doc-catalog/domain/chorus" 2>/dev/null)
  [ $? -eq 0 ]
  has_arrays=$(echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); print('yes' if isinstance(d.get('governs'), list) and isinstance(d.get('references'), list) else 'no')" 2>/dev/null)
  [ "$has_arrays" = "yes" ]
}

# --- AC 5: Artifacts linked to governing domains ---
#
# #3606 — these WRITES need a token now. The security envelope (#3618/#3719)
# started requiring identity on mutating endpoints after these were written, so
# both POSTs got 401 — and the assertions read that as "the endpoint is broken".
# It isn't: with a minted token the invalid-relationship case returns exactly the
# 400 test 2 expects. Verified live before changing anything.
_token() { bash "${CHORUS_ROOT}/platform/scripts/chorus-identity-token" kade 2>/dev/null | head -1; }

@test "POST /api/doc-catalog/link creates a governs edge" {
  local tok; tok=$(_token)
  [ -n "$tok" ] || skip "no identity token available on this host"
  http_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$CHORUS_API/api/doc-catalog/link" \
    -H "Authorization: Bearer $tok" \
    -H "Content-Type: application/json" \
    -d '{"href": "/gathering-docs/service-design-knowledge.html", "domain": "chorus", "relationship": "governs"}' 2>/dev/null)
  [ "$http_code" = "201" ] || [ "$http_code" = "200" ] || [ "$http_code" = "409" ]
}

@test "POST /api/doc-catalog/link rejects invalid relationship" {
  local tok; tok=$(_token)
  [ -n "$tok" ] || skip "no identity token available on this host"
  http_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$CHORUS_API/api/doc-catalog/link" \
    -H "Authorization: Bearer $tok" \
    -H "Content-Type: application/json" \
    -d '{"href": "/test.html", "domain": "chorus", "relationship": "invalid"}' 2>/dev/null)
  [ "$http_code" = "400" ]
}

# --- Artifact type classification ---

@test "doc-catalog entries include artifactType field" {
  result=$(curl -sf "$CHORUS_API/api/doc-catalog" 2>/dev/null)
  has_type=$(echo "$result" | python3 -c "
import json,sys
d=json.load(sys.stdin)
groups = d.get('groups',[])
for g in groups:
    for doc in g.get('docs',[]):
        if doc.get('artifactType'):
            print('yes')
            sys.exit(0)
print('no')
" 2>/dev/null)
  [ "$has_type" = "yes" ]
}
