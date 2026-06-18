#!/bin/bash
# owl-api generated-route conformance walker (#3354 AC4) — for a LIVE server,
# walks every generated route and validates: valid JSON, expected shape, and
# that telemetry covers the walk (the generated API proving itself).
set -uo pipefail
PORT="${OWL_API_PORT:-3360}"
BASE="http://localhost:$PORT"
PASS=0; FAIL=0
ok(){ PASS=$((PASS+1)); echo "  ok  - $1"; }
bad(){ FAIL=$((FAIL+1)); echo "  FAIL - $1"; }

curl -sf -m 5 "$BASE/health" | grep -q '"ok": true' && ok "/health answers" || bad "/health"
LIST=$(curl -sf -m 20 "$BASE/domains")
echo "$LIST" | python3 -m json.tool >/dev/null 2>&1 && ok "/domains is valid JSON" || bad "/domains JSON"
COUNT=$(echo "$LIST" | python3 -c "import json,sys; print(json.load(sys.stdin)['count'])" 2>/dev/null || echo 0)
[ "$COUNT" -ge 1 ] && ok "/domains count=$COUNT" || bad "/domains empty"
NAME=$(echo "$LIST" | python3 -c "import json,sys; print(json.load(sys.stdin)['items'][0]['name'])" 2>/dev/null)
DETAIL=$(curl -sf -m 20 "$BASE/domains/$NAME")
echo "$DETAIL" | python3 -c "
import json,sys
d=json.load(sys.stdin)
missing=[f for f in ('iri','type','label','created','creator') if f not in d]
sys.exit(1 if missing else 0)" && ok "/domains/$NAME carries shape surface + audit" || bad "/domains/$NAME shape"
curl -sf -m 20 "$BASE/domains/$NAME/contains" | python3 -m json.tool >/dev/null 2>&1 && ok "fold route answers" || bad "fold route"
curl -s -m 20 -o /dev/null -w "%{http_code}" "$BASE/domains/zz-no-such" | grep -q 404 && ok "unknown entity → 404 (typed refusal)" || bad "404 path"

# ── contract validation (#3364 AC3): every detail-response field must be ──
# ── declared in the generated OpenAPI spec — undocumented fields fail.   ──
# #3488 — the generated OpenAPI baseline now lands at its model-declared home
# (chorus:repoTarget → designing/products/athena/domains/domains), not the old
# hardcoded platform/services/owl-api/generated/. Repointed as part of the
# repoTarget migration (old generated/ retired). Follow-on: derive this via
# `owl-api generate-target --class Domain` so the path stays config-as-data.
SPEC="$(cd "$(dirname "$0")/../.." && pwd)/designing/products/athena/domains/domains/openapi.json"
if [ -f "$SPEC" ]; then
  echo "$DETAIL" | python3 -c "
import json,sys
d=json.load(sys.stdin)
spec=json.load(open('$SPEC'))
declared=set(spec['components']['schemas']['Domain']['properties'].keys())
undocumented=[k for k in d.keys() if k not in declared]
missing_required=[k for k in ('iri','created','creator') if k not in d]
if undocumented: print('  undocumented fields (not in contract):', undocumented)
if missing_required: print('  missing required audit fields:', missing_required)
sys.exit(1 if (undocumented or missing_required) else 0)" \
    && ok "/domains/$NAME validates against generated OpenAPI contract" \
    || bad "contract validation"
else
  bad "contract validation (no designing/products/athena/domains/domains/openapi.json baseline)"
fi
echo "════ $PASS passed, $FAIL failed"
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
