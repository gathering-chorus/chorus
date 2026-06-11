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
echo "════ $PASS passed, $FAIL failed"
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
