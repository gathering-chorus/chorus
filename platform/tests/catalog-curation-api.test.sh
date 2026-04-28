#!/usr/bin/env bash
# Live tests for #2549: doc-catalog write API — 5-field tags + lineage edges.
#
# Verifies curation operations land in Athena (urn:chorus:instances graph) and
# read back through the GET endpoints. Test fixtures use unique hrefs per run
# (timestamp-suffixed) so reruns are hermetic against accumulated test data.

set -uo pipefail

API_BASE="${API_BASE:-http://localhost:3340}"
TAGS_URL="$API_BASE/api/chorus/catalog/tags"
LINEAGE_URL="$API_BASE/api/chorus/catalog/lineage"
DOC_URL="$API_BASE/api/chorus/catalog/doc"
DRIFT_URL="$API_BASE/api/chorus/catalog/drift"

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

TS=$(date +%s)
HREF_A="/test/catalog-curation/doc-a-${TS}.html"
HREF_B="/test/catalog-curation/doc-b-${TS}.html"

# Helper: url-safe base64 of href (path-safe identity for GET /doc/:id)
href_id() { python3 -c "import base64,sys; print(base64.urlsafe_b64encode(sys.argv[1].encode()).decode().rstrip('='))" "$1"; }

# ---------------------------------------------------------------------------
# AC1 — POST /catalog/tags writes five-field tag, emits curated event.

# 1. POST with missing href is rejected at boundary.
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -d '{"product":"chorus"}' "$TAGS_URL")
check "POST tags without href rejected at 400" "400" "$CODE"

# 2. POST with valid 5-field tag returns 200 and persisted state.
POST_BODY=$(cat <<EOF
{"href":"$HREF_A","product":"chorus","subproduct":"loom","domain":"chorus","subdomain":"loom-principles","role":"wren"}
EOF
)
RESP=$(curl -s -X POST -H 'Content-Type: application/json' -d "$POST_BODY" "$TAGS_URL")
HTTP_OK=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "$POST_BODY" "$TAGS_URL")
check "POST tags valid body returns 200" "200" "$HTTP_OK"
PERSISTED_PRODUCT=$(echo "$RESP" | python3 -c "import json,sys; print(json.load(sys.stdin).get('data',{}).get('product',''))" 2>/dev/null || echo "")
check "POST tags returns persisted product" "chorus" "$PERSISTED_PRODUCT"

# 3. POST with unknown vocab rejected at 400.
BAD_BODY=$(cat <<EOF
{"href":"$HREF_A","product":"chorus","subdomain":"definitely-not-a-real-subdomain"}
EOF
)
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "$BAD_BODY" "$TAGS_URL")
check "POST tags unknown subdomain rejected at 400" "400" "$CODE"

# ---------------------------------------------------------------------------
# AC3 — GET /catalog/doc/:hrefb64 returns five-field tags.

ID_A=$(href_id "$HREF_A")
DOC_RESP=$(curl -s "$DOC_URL/$ID_A")
DOC_PRODUCT=$(echo "$DOC_RESP" | python3 -c "import json,sys; print(json.load(sys.stdin).get('data',{}).get('tags',{}).get('product',''))" 2>/dev/null || echo "")
check "GET doc returns persisted product" "chorus" "$DOC_PRODUCT"
DOC_SUBDOMAIN=$(echo "$DOC_RESP" | python3 -c "import json,sys; print(json.load(sys.stdin).get('data',{}).get('tags',{}).get('subdomain',''))" 2>/dev/null || echo "")
check "GET doc returns persisted subdomain" "loom-principles" "$DOC_SUBDOMAIN"
DOC_ROLE=$(echo "$DOC_RESP" | python3 -c "import json,sys; print(json.load(sys.stdin).get('data',{}).get('tags',{}).get('role',''))" 2>/dev/null || echo "")
check "GET doc returns persisted role" "wren" "$DOC_ROLE"

# 404 on unknown doc.
CODE=$(curl -s -o /dev/null -w '%{http_code}' "$DOC_URL/$(href_id /nonexistent-doc-xyz)")
check "GET doc unknown returns 404" "404" "$CODE"

# ---------------------------------------------------------------------------
# AC2 — POST /catalog/lineage writes edge, bidirectional render.

# Create doc B so we can link A → B.
POST_B=$(cat <<EOF
{"href":"$HREF_B","product":"chorus","subproduct":"loom","domain":"chorus","subdomain":"loom-principles","role":"wren"}
EOF
)
curl -s -o /dev/null -X POST -H 'Content-Type: application/json' -d "$POST_B" "$TAGS_URL"

# A supersedes B
LINEAGE_BODY=$(cat <<EOF
{"subject_href":"$HREF_A","predicate":"supersedes","object_href":"$HREF_B"}
EOF
)
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "$LINEAGE_BODY" "$LINEAGE_URL")
check "POST lineage valid edge returns 200" "200" "$CODE"

# Invalid predicate rejected.
BAD_LIN=$(cat <<EOF
{"subject_href":"$HREF_A","predicate":"notARealPredicate","object_href":"$HREF_B"}
EOF
)
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "$BAD_LIN" "$LINEAGE_URL")
check "POST lineage invalid predicate rejected at 400" "400" "$CODE"

# Bidirectional read: GET doc A shows out-edge supersedes→B; GET doc B shows in-edge supersedes←A.
DOC_A=$(curl -s "$DOC_URL/$(href_id "$HREF_A")")
A_OUT=$(echo "$DOC_A" | python3 -c "import json,sys; d=json.load(sys.stdin).get('data',{}); print(len([e for e in d.get('lineage',{}).get('out',[]) if e.get('predicate')=='supersedes' and e.get('object_href')=='$HREF_B']))" 2>/dev/null || echo 0)
check "GET doc A shows supersedes out-edge to B" "1" "$A_OUT"

DOC_B=$(curl -s "$DOC_URL/$(href_id "$HREF_B")")
B_IN=$(echo "$DOC_B" | python3 -c "import json,sys; d=json.load(sys.stdin).get('data',{}); print(len([e for e in d.get('lineage',{}).get('in',[]) if e.get('predicate')=='supersedes' and e.get('subject_href')=='$HREF_A']))" 2>/dev/null || echo 0)
check "GET doc B shows supersedes in-edge from A" "1" "$B_IN"

# ---------------------------------------------------------------------------
# AC4 — GET /catalog/drift returns path↔tag divergences.

# Doc A's path implies product=chorus (path under /test/catalog-curation/), tags say chorus → no drift on this fixture.
# Use a known-divergent fixture: href under gathering-docs/... but tags say product=chorus.
# (tagsFromPath checks startsWith on lowercased path after stripping leading slash.)
HREF_DIVERGE="gathering-docs/curation-divergence-${TS}.html"
DIV_BODY=$(cat <<EOF
{"href":"$HREF_DIVERGE","product":"chorus","subproduct":"loom","domain":"chorus","subdomain":"loom-principles","role":"wren"}
EOF
)
curl -s -o /dev/null -X POST -H 'Content-Type: application/json' -d "$DIV_BODY" "$TAGS_URL"

DRIFT_RESP=$(curl -s "$DRIFT_URL")
N=$(echo "$DRIFT_RESP" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len([e for e in d.get('data',{}).get('drift',[]) if e.get('href')=='$HREF_DIVERGE']))" 2>/dev/null || echo 0)
check "GET drift surfaces path↔tag divergence" "1" "$N"

# ---------------------------------------------------------------------------
# AC #2554 — SHACL shape catches malformed CatalogDoc.
# Insert a malformed instance directly via SPARQL (bypass API vocab validation),
# verify /api/athena/validate surfaces it, then clean up.

FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
VALIDATE_URL="$API_BASE/api/athena/validate"
MALFORMED_URI="https://jeffbridwell.com/chorus#catalog-doc-shape-violation-${TS}"

# Insert: CatalogDoc with no catalogHref (violates required-prop shape).
curl -s -o /dev/null -X POST -H 'Content-Type: application/sparql-update' \
  --data "PREFIX chorus: <https://jeffbridwell.com/chorus#>
INSERT DATA { GRAPH <urn:chorus:instances> { <$MALFORMED_URI> a chorus:CatalogDoc ; chorus:product \"chorus\" } }" \
  "$FUSEKI_UPDATE"

VIOLATIONS=$(curl -s "$VALIDATE_URL" | python3 -c "
import json, sys
d = json.load(sys.stdin)
hits = [v for v in d.get('violations', []) if 'catalog-doc-shape-violation-${TS}' in v.get('node','') or 'CatalogDoc' in v.get('constraint','')]
print(len(hits))
" 2>/dev/null || echo 0)
check "validate surfaces malformed CatalogDoc (no catalogHref)" "1" "$([ "$VIOLATIONS" -ge 1 ] && echo 1 || echo 0)"

# Cleanup
curl -s -o /dev/null -X POST -H 'Content-Type: application/sparql-update' \
  --data "PREFIX chorus: <https://jeffbridwell.com/chorus#>
DELETE WHERE { GRAPH <urn:chorus:instances> { <$MALFORMED_URI> ?p ?o } }" \
  "$FUSEKI_UPDATE"

# ---------------------------------------------------------------------------
# Cleanup is best-effort; test docs use namespaced /test/ hrefs that won't
# collide with real catalog entries. A separate sweep card can prune by prefix.

echo
echo "Catalog curation API tests: $pass passed, $fail failed."
[ "$fail" -eq 0 ]
