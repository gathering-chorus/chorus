#!/bin/bash
# athena-serve-prove — #3846. The proving step the athena pipeline never had.
#
# THE GAP THIS CLOSES (measured 2026-08-12):
# The athena verb chain is model -> make -> deploy. It stops at deploy. Nothing
# asserts the SERVED surface answers what the model CLAIMS — so a claimed surface
# can be declared, generated, deployed, and never actually reachable, and no gate
# goes red. That is exactly how /effective (the #3435 properties-cascade handler)
# sat in the owl-api source for two months, compiled into the running binary, and
# still 404'd: select_table matches only class plurals, /effective matches none,
# and no check ever asked "does the thing we claim to serve actually serve?".
#
# DECLARED superset CLAIMED superset SERVED. athena-validate.ts proves the GRAPH
# (SHACL, model-side). owl-api-conformance.sh proves ONE domain over HTTP. Neither
# proves the FULL claimed set is served. This does: for each claimed surface it
# asserts a live answer over :3360, and FAILS on any claimed-but-unserved route.
#
# NEGATIVE PROOF (#3734) — this check is not hollow. It ships pointed at a REAL,
# LIVE violation: /effective is claimed by the #3435 handler and is unserved today
# (dispatch bug, Wren's #3845). So this check goes RED right now, against the exact
# state it exists to catch — not a synthetic fixture. It goes green when #3845
# lands the dispatch fix. A serve-prove that could not currently go red would be
# the very thing it is built to forbid.
#
# THREE STATES IT KEEPS DISTINCT (never collapses two into one):
#   UNREACHABLE (exit 2) — :3360 itself is down. "can't tell", not "unserved".
#   VIOLATION   (exit 1) — server up, a claimed route 404s. real claimed-unserved.
#   PROVEN      (exit 0) — server up, every claimed route answers.
set -uo pipefail
PORT="${OWL_API_PORT:-3360}"
BASE="http://localhost:$PORT"

# ── gate on reachability FIRST, so a down server can never read as a pass ──
if ! curl -sf -m 5 "$BASE/health" | grep -q '"ok": true'; then
  echo "UNREACHABLE — :$PORT/health does not answer. Cannot prove serving; not a pass."
  exit 2
fi

# ── the SERVED contract: what routes.json (the generated manifest) says it serves.
#    Every plural in the served list must actually answer its collection route.   ──
SERVED_JSON="$(curl -sf -m 20 "$BASE/domains")" || { echo "UNREACHABLE — /domains errored"; exit 2; }

# ── the CLAIMED set. Two sources, deliberately NOT routes.json alone (routes.json
#    IS the served manifest — checking served-against-served is the hollow trap that
#    let /effective hide). Claimed = the generated collection routes the server
#    advertises AS served, PLUS the model-declared cross-cutting surfaces that are
#    hand-wired handlers, not per-class CRUD (today: the properties cascade). ──
CLAIMED_COLLECTIONS="$(curl -sf -m 10 "$BASE/domains/zz-probe-unknown" -o /dev/null -w '%{http_code}')"
[ "$CLAIMED_COLLECTIONS" = "404" ] || { echo "VIOLATION — unknown entity did not 404 (typed-refusal contract broken)"; exit 1; }

VIOL=0
proven(){ echo "  proven  - $1"; }
violation(){ VIOL=$((VIOL+1)); echo "  VIOLATION - $1"; }

# 1) every CLAIMED domain (the model's domain list under /domains) must actually
#    answer its collection route. The route segment is the domain's `label`
#    (lowercase plural the generator serves at). A claimed domain whose collection
#    route 404s is claimed-but-unserved — the whole failure class.
ROUTES="$(echo "$SERVED_JSON" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("\n".join((i.get("label") or "").strip() for i in d.get("data",[]) if i.get("label")))' 2>/dev/null)"
[ -n "$ROUTES" ] || { echo "VIOLATION — /domains served an empty claimed-domain set"; exit 1; }
while IFS= read -r n; do
  [ -z "$n" ] && continue
  code="$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$BASE/$n")"
  if [ "$code" = "200" ]; then proven "domain /$n serves ($code)"; else violation "domain /$n claimed by model, collection route serves $code"; fi
done <<< "$ROUTES"

# 2) the model-declared cross-cutting surfaces (not per-class CRUD). The properties
#    cascade: #3435 declares GET /effective/:node/:key. This is the live negative
#    proof — it 404s today (dispatch bug), so this assertion is EXPECTED red until
#    #3845 lands. That red is the point: it is the state serve-prove exists to catch.
EFF_CODE="$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$BASE/effective/role-silas/response.word.cap")"
# unset (404 with a typed unset body) is a valid SERVED answer; unknown-route 404 is NOT.
if [ "$EFF_CODE" = "404" ]; then
  BODY="$(curl -s -m 15 "$BASE/effective/role-silas/response.word.cap")"
  if echo "$BODY" | grep -q '"unknown route"'; then
    violation "claimed surface /effective/:node/:key is UNSERVED (unknown route) — #3435 handler unreachable, #3845 dispatch fix pending"
  else
    proven "/effective/:node/:key serves (404 = typed unset, a real answer)"
  fi
elif [ "$EFF_CODE" = "200" ] || [ "$EFF_CODE" = "400" ]; then
  proven "/effective/:node/:key serves ($EFF_CODE)"
else
  violation "claimed surface /effective/:node/:key returned $EFF_CODE"
fi

echo
if [ "$VIOL" -eq 0 ]; then
  echo "PROVEN — every claimed surface answers over :$PORT."
  exit 0
else
  echo "VIOLATION — $VIOL claimed surface(s) declared/deployed but not served. The athena pipeline stopped at deploy."
  exit 1
fi
