#!/usr/bin/env bats
# @test-type: integration — hermetic TTL guards (arq) + live owl-api recursive-tree
# serve check (service-hitting, skip-if-absent per #3528).
load test_helper
#
# #3699 — recursive value-stream tree. What Jeff sees: GET /valuestreams/chorus/tree
# returns the fractal — Chorus's 6 steps, each composing its child stream, live from
# owl-api. AC1 fix: chorus:hasValueStream was REFERENCED (ValueStreamShape treeEdge +
# the composition edges in value-stream-instances.ttl) but only DECLARED in the
# superseded chorus-core.ttl; it is now defined as owl:ObjectProperty in the live
# chorus.ttl so the property the recursion rides is not a dangling reference.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CT="$REPO/roles/silas/ontology/chorus.ttl"
  VSI="$REPO/designing/data/value-stream-instances.ttl"
  VSS="$REPO/designing/data/value-stream-step-instances.ttl"   # #3904 — the #3839 split
  # #3991 repoint, missed here: #3561 renamed chorus-model-deploy.sh →
  # athena-deploy-model.sh.
  DEPLOY="$REPO/platform/scripts/athena-deploy-model.sh"
  OWL_URL="${OWL_URL:-http://localhost:3360}"
}

aq() { # arq over a data file; $1=data $2=query
  local qf; qf="$(mktemp "${BATS_TMPDIR:-/tmp}/tq.XXXXXX.rq")"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\nPREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n%s\n' "$2" > "$qf"
  arq --data="$1" --data="$VSS" --query="$qf" 2>/dev/null; rm -f "$qf"   # #3904: grammar spans two files since #3839
}

@test "arq SPARQL engine is present (no false-green from a missing binary)" {
  command -v arq
}

# ── AC1: hasValueStream is DEFINED (owl:ObjectProperty) in the live model, not dangling ──
@test "AC1 chorus:hasValueStream is declared owl:ObjectProperty in chorus.ttl" {
  run aq "$CT" 'ASK { c:hasValueStream a owl:ObjectProperty }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 hasValueStream domain=ValueStreamStep, range=ValueStream" {
  run aq "$CT" 'ASK { c:hasValueStream rdfs:domain c:ValueStreamStep ; rdfs:range c:ValueStream }'
  echo "$output" | grep -qi 'yes'
}

# ── AC2: composition edges present — each Chorus step composes a child stream ──
@test "AC2 all 6 Chorus steps carry a hasValueStream composition edge" {
  run aq "$VSI" 'SELECT (COUNT(*) AS ?n) WHERE { ?s c:inStream c:value-stream-chorus ; c:hasValueStream ?child }'
  echo "$output" | grep -qE '\|[[:space:]]*6[[:space:]]*\|'
}
@test "AC2 every hasValueStream target is a declared ValueStream (no dangling child)" {
  run aq "$VSI" 'SELECT (COUNT(*) AS ?dangling) WHERE { ?s c:hasValueStream ?child . FILTER NOT EXISTS { ?child a c:ValueStream } }'
  echo "$output" | grep -qE '\|[[:space:]]*0[[:space:]]*\|'
}

# ── AC4: the staging draft is INGESTED (via #3698 INSTANCE_SET), not un-ingested ──
@test "AC4 value-stream-instances.ttl is ingested via the seed manifest (#3895 lane)" {
  # #3904 — the INSTANCE_SET lane moved OUT of chorus-model-deploy.sh (#3895:
  # the recovery path must not carry the DAL-gated seed). The ingest lane is
  # now `athena-model seed --deploy` reading the committed manifest.
  MANIFEST="$REPO/platform/config/instance-seed-manifest.txt"
  [ -f "$MANIFEST" ]
  grep -q 'value-stream-instances.ttl' "$MANIFEST"
  grep -q 'value-stream-step-instances.ttl' "$MANIFEST"
}

# ── AC3 (live): GET /valuestreams/chorus/tree returns the RECURSIVE tree ──
# A real recursion = a grandchild depth: chorus → step → child-stream → child-step.
@test "AC3 GET /valuestreams/value-stream-chorus/tree serves a nested (recursive) tree" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$OWL_URL/valuestreams/value-stream-chorus/tree"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$OWL_URL/valuestreams/value-stream-chorus/tree"
  # depth-3 presence proves recursion (root has children, and a child has its own children)
  local depth; depth=$(echo "$output" | python3 -c "
import sys,json
d=json.load(sys.stdin)
def maxdepth(n):
    ch=n.get('children') or []
    return 1+(max((maxdepth(c) for c in ch), default=0))
print(maxdepth(d))
" 2>/dev/null || echo 0)
  [ "${depth:-0}" -ge 3 ]
}
