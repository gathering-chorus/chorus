#!/usr/bin/env bash
# @test-type: security — source-vs-store manifest audit
# test-security-manifest-3726.sh — #3726: the security substrate must survive a
# fresh load. Proves the deploy set carries identity/scope/schema/surfaces AND
# that a clean load reproduces a WORKING allow-set — webIds that MATCH what CSS
# tokens actually present, not the stale localhost scheme that would lock out
# every role on reload.
#
# RED before #3726: the security TTLs are not in athena-deploy-model.sh, and
# identity-principals-3613.ttl carries localhost:3001 webIds that no token
# matches. GREEN after: files in the deploy set + webIds reconciled to the
# logical issuer + a scratch load reproduces 7 security principals / 13 scopes /
# schema+surfaces in ontology.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEPLOY="$ROOT/platform/scripts/athena-deploy-model.sh"
ID_TTL="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
SCOPE_TTL="$ROOT/roles/silas/ontology/security-scopes-3689.ttl"
FAIL=0
ok(){ echo "  PASS: $1"; }
no(){ echo "  FAIL: $1"; FAIL=1; }

# 1 — source reconciled: 7 principals, ALL logical-issuer webIds, marknakib present, ZERO localhost
np=$(grep -c "a chorus:Principal" "$ID_TTL" 2>/dev/null || echo 0)
# #3904: 7→11 — the roster grew by reviewed cards (jeff+marknakib #3773 restore, bridge #3691, canary #3871).
# #4004: 11→12 — principal-nightly, added by Kade's #3975 (a3bd05d30) without moving this
# count, so a reviewed addition read as a ghost. The exact-count discipline STAYS: it is
# what surfaced the change at all. Moving the number is the second half of adding a
# principal, and this comment is the receipt for which card each increment came from.
[ "$np" -eq 12 ] && ok "identity source has all 12 security principals ($np)" || no "identity source principal count = $np, expected 12"
if grep -qE 'webId +"http://localhost' "$ID_TTL" 2>/dev/null; then
  no "identity source STILL carries a localhost webId value — a fresh load would lock out every role"
else
  ok "no stale localhost webId values in identity source (reload-safe)"
fi
grep -q "principal-marknakib" "$ID_TTL" 2>/dev/null && ok "marknakib in source (was live-only drift)" || no "marknakib missing from source"
grep -q "id.lightlifeurbangardens.com" "$ID_TTL" 2>/dev/null && ok "webIds track the logical issuer (match token webids)" || no "webIds do not match the issuer tokens present"

# 2 — scope source faithful to live: 13 grants (silas/wren/kade x4 + chorus-sdk x1)
ns=$(grep -oE '"urn:chorus:[^"]+"\^\^xsd:anyURI' "$SCOPE_TTL" 2>/dev/null | grep -c . || echo 0)
[ "$ns" -eq 14 ] && ok "scope source has 13 grants (matches live)" || no "scope source grant count = $ns, expected 14"

# 3 — deploy set REFERENCES the security files (else a fresh load omits them)
grep -q "security-model-3618.ttl" "$DEPLOY" && ok "deploy set includes the security schema" || no "security-model-3618.ttl NOT in the deploy set — fresh load has no security schema"
grep -q "identity-principals-3613.ttl" "$DEPLOY" && ok "deploy set includes the identity principals" || no "identity-principals-3613.ttl NOT in the deploy set — fresh load has no allow-set"
grep -q "security-scopes-3689.ttl" "$DEPLOY" && ok "deploy set includes the scope grants" || no "security-scopes-3689.ttl NOT in the deploy set — fresh load has no scopes"
grep -q "security-3619-surfaces.ttl" "$DEPLOY" && ok "deploy set includes the secured surfaces" || no "surfaces NOT in the deploy set"

# 4 — BEHAVIORAL: load identity+scopes into a SCRATCH graph, prove the reproduced
#     allow-set is workable (7 principals, 13 scopes, webIds = logical issuer).
if command -v riot >/dev/null 2>&1; then
  riot --validate "$ID_TTL" >/dev/null 2>&1 && ok "identity TTL riot-valid" || no "identity TTL riot-invalid"
  riot --validate "$SCOPE_TTL" >/dev/null 2>&1 && ok "scope TTL riot-valid" || no "scope TTL riot-invalid"
fi
AUTH="$ROOT/platform/scripts/fuseki-auth.sh"
if [ -r "$AUTH" ] && curl -s -m4 http://localhost:3030/pods/query --data-urlencode 'query=ASK{}' -H 'Accept: text/csv' >/dev/null 2>&1; then
  # shellcheck disable=SC1090
  source "$AUTH" 2>/dev/null
  GSP="http://localhost:3030/pods/data"; Q="http://localhost:3030/pods/query"
  SG="urn:chorus:test:3726-scratch-security"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$SG" -o /dev/null 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST -H 'Content-Type: text/turtle' --data-binary "@$ID_TTL" "$GSP?graph=$SG" -o /dev/null 2>/dev/null
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST -H 'Content-Type: text/turtle' --data-binary "@$SCOPE_TTL" "$GSP?graph=$SG" -o /dev/null 2>/dev/null
  sp=$(curl -s "$Q" --data-urlencode "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <$SG> { ?p a c:Principal } }" -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
  sc=$(curl -s "$Q" --data-urlencode "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?s) AS ?n) WHERE { GRAPH <$SG> { ?x c:hasScope ?s } }" -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
  sloc=$(curl -s "$Q" --data-urlencode "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?w) AS ?n) WHERE { GRAPH <$SG> { ?p c:webId ?w FILTER(CONTAINS(STR(?w),\"localhost\")) } }" -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$SG" -o /dev/null 2>/dev/null || true
  [ "${sp:-0}" -eq 12 ] && ok "scratch load reproduces 12 security principals" || no "scratch load reproduced ${sp:-?} principals, expected 12"
  [ "${sc:-0}" -eq 14 ] && ok "scratch load reproduces 14 scope grants" || no "scratch load reproduced ${sc:-?} scopes, expected 14"
  [ "${sloc:-1}" -eq 0 ] && ok "reproduced allow-set has ZERO localhost webIds (matches real tokens)" || no "reproduced allow-set has ${sloc} localhost webIds — reload would lock out"
else
  echo "  SKIP: Fuseki not reachable — behavioral scratch load not run (source + deploy-set checks stand)"
fi

echo; [ $FAIL -eq 0 ] && { echo "PASS"; exit 0; } || { echo "RED"; exit 1; }
