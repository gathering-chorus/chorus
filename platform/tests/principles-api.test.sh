#!/usr/bin/env bash
# Live tests for #2314: principles API (POST/PUT/DELETE) round-trip.
# Verifies CRUD lands in instances graph and is visible on all three surfaces:
# Athena canonical, Athena subdomain detail, and the Loom 308 redirect.
set -uo pipefail

API_BASE="${API_BASE:-http://localhost:3340}"
WRITE_URL="${PRINCIPLES_WRITE_URL:-$API_BASE/api/athena/subdomains/loom-principles/principles}"
READ_URL_CANONICAL="$API_BASE/api/athena/subdomains/loom-principles/principles"
READ_URL_LOOM_REDIRECT="$API_BASE/api/loom/principles"
READ_URL_ATHENA_DETAIL="$API_BASE/api/athena/subdomains/loom-principles"

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

TS=$(date +%s)
TEST_LABEL="Test principle ${TS}"
POST_BODY="{\"label\":\"$TEST_LABEL\",\"comment\":\"hermetic test principle — safe to delete\"}"

# 0. POST without required label is rejected at the boundary (400). Full
# cross-graph SHACL validation lands in #2469's gate test.
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -d '{"comment":"missing-label probe"}' "$WRITE_URL")
check "POST without label rejected at 400" "400" "$CODE"

# 1. POST creates
RESP=$(curl -s -X POST -H 'Content-Type: application/json' -d "$POST_BODY" "$WRITE_URL")
URI=$(echo "$RESP" | python3 -c "import json,sys; print(json.load(sys.stdin)['data'].get('uri',''))" 2>/dev/null || echo "")
check "POST returned a URI" "1" "$([ -n "$URI" ] && echo 1 || echo 0)"
ENTITY_ID="${URI##*#}"

# 2. canonical Athena GET sees it
N=$(curl -s "$READ_URL_CANONICAL" | grep -c "$TEST_LABEL")
check "visible on Athena canonical principles list" "1" "$([ "$N" -gt 0 ] && echo 1 || echo 0)"

# 3. Loom redirect serves the same
N=$(curl -sL "$READ_URL_LOOM_REDIRECT" | grep -c "$TEST_LABEL")
check "visible via /api/loom/principles 308 redirect" "1" "$([ "$N" -gt 0 ] && echo 1 || echo 0)"

# 4. Athena subdomain detail surfaces it (cross-graph UNION read)
N=$(curl -s "$READ_URL_ATHENA_DETAIL" | grep -c "$TEST_LABEL")
check "visible on /api/athena/subdomains/loom-principles" "1" "$([ "$N" -gt 0 ] && echo 1 || echo 0)"

# 5. PUT updates idempotently
UPDATED_LABEL="${TEST_LABEL} updated"
curl -s -o /dev/null -X PUT -H 'Content-Type: application/json' \
  -d "{\"label\":\"$UPDATED_LABEL\"}" "$WRITE_URL/$ENTITY_ID"
N=$(curl -s "$READ_URL_CANONICAL" | grep -c "$UPDATED_LABEL")
check "PUT updates label (visible on read)" "1" "$([ "$N" -gt 0 ] && echo 1 || echo 0)"

# 6. DELETE removes
curl -s -o /dev/null -X DELETE "$WRITE_URL/$ENTITY_ID"
N=$(curl -s "$READ_URL_CANONICAL" | grep -c "$TEST_LABEL")
check "DELETE removes (not visible on read)" "0" "$N"

# 7. Loom GET returns 308 (not 302/301) and Location is the canonical Athena path.
# Guards against the redirect silently flipping status code or target.
REDIRECT_INFO=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' "$READ_URL_LOOM_REDIRECT")
REDIRECT_CODE=$(echo "$REDIRECT_INFO" | awk '{print $1}')
REDIRECT_TARGET=$(echo "$REDIRECT_INFO" | awk '{print $2}')
check "Loom GET status is 308 (permanent redirect)" "308" "$REDIRECT_CODE"
check "Loom GET Location points to canonical Athena path" "$READ_URL_CANONICAL" "$REDIRECT_TARGET"

# 8. Cross-graph UNION (subdomain-detail.sparql): SubDomain still lives in
# urn:chorus:ontology, Principle instances now live in urn:chorus:instances. The
# detail query UNIONs both graphs so the subdomain page surfaces 27 contained
# Principle instances. When SubDomain migrates last under #2469, this test is
# the canary that the read path didn't break.
INSTANCE_COUNT=$(curl -s "$READ_URL_ATHENA_DETAIL" | python3 -c "
import json, sys
insts = json.load(sys.stdin)['data'].get('instances', [])
principles = [i for i in insts if i.get('type') == 'Principle']
print(len(principles))
" 2>/dev/null || echo 0)
check "cross-graph UNION returns 27 Principle instances on subdomain detail" "27" "$INSTANCE_COUNT"

# 9. chorus:order propagates to API → page render. Permaculture parents must come
# back as the first 14 entries of the principles list, ordered 1..14 by chorus:order.
ORDER_OK=$(curl -s "$READ_URL_CANONICAL" | python3 -c "
import json, sys
ps = json.load(sys.stdin)['data']['principles']
parents = [p for p in ps if p['isPermacultureParent']]
orders = [p.get('order') for p in parents]
expected = list(range(1, 15))
print(1 if orders == expected else 0)
" 2>/dev/null || echo 0)
check "permaculture parents return in book order 1..14" "1" "$ORDER_OK"

# 10. Multi-parent specialization: 'no competing implementations' is upstreamed
# to parents 5, 6, and 7. API response must include all three parents on that
# child so the page can nest it under each.
MULTI_OK=$(curl -s "$READ_URL_CANONICAL" | python3 -c "
import json, sys
ps = json.load(sys.stdin)['data']['principles']
nc = next((p for p in ps if 'no-competing' in p.get('id','')), None)
if not nc: print(0); sys.exit()
labels = sorted(pp['label'] for pp in nc.get('parents', []))
expected = sorted([
  'Each function is supported by multiple elements',
  'Make the least change for the greatest effect',
  'Use small-scale, intensive systems',
])
print(1 if labels == expected else 0)
" 2>/dev/null || echo 0)
check "multi-parent: 'no competing' lists all 3 upstream parents" "1" "$MULTI_OK"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
