#!/usr/bin/env bash
# enrichment-write-fileInDomain.sh — #3017: function-based belongs-to writer.
#
# RETIRES the path-regex SUBDOMAIN_MAP (the prior #2844 version inferred a
# file's domain from its directory prefix — over-broad, and exactly the
# "repo tree != domains" trap). Domain membership is a FUNCTIONAL judgment:
# a file belongs-to a domain when it IS that domain's surface (defines /
# implements / tests it), NOT because of where it sits in the tree.
#
# fileInDomain = domain-radius (what a file IS). Its sibling
# enrichment-write-fileDependsOn.sh = blast-radius (what a file USES).
#
# BELONGS_MAP is the owner-confirmed core per domain: <rel-path>|<domain>|<owner>,
# each entry a file whose FUNCTION constitutes the domain. This is the
# "owner confirms" half of the tagging loop (auto-propose-from-define-signals
# is the generalization, future). Adding a domain's core is a few one-line
# edits, no code change. Seeded + validated on chorus:spine.
#
# Idempotent per matched file: DELETE its fileInDomain/fileHasOwner edges,
# then INSERT the detected set. writeOwner=chorus:enrichment.
#
# Spine event: enrichment.fileInDomain.written {files, edges, duration_ms, failures}

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-$FUSEKI_BASE/update}"
HYDRATION_GRAPH="${HYDRATION_GRAPH:-urn:chorus:instances}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# <rel-path of a file that IS the domain, by function>|<domain>|<owner-role>
# spine core: write-lib + spine's own tests. Excluded: server.ts (multi-domain
# hub — hosts spine read-tools but isn't wholly spine); chorus-log (a symlink
# to the generic shim-wrapper.sh → rust binary, so the emit impl is
# binary-backed, a different artifact class, not a taggable single source here).
BELONGS_MAP=(
  'platform/api/src/spine-event-write.ts|spine|role-wren'
  'platform/api/tests/spine-event-endpoint.integration.test.ts|spine|role-wren'
  'platform/api/tests/spine-event-write.test.ts|spine|role-wren'
  'platform/tests/spine-emit-drift-audit.bats|spine|role-wren'
  'platform/tests/spine-tick-poller-inject-resolve.bats|spine|role-wren'
)

post_update() {
  curl -s -o /dev/null -w '%{http_code}' \
    -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "$1" "$FUSEKI_UPDATE" 2>/dev/null || echo "000"
}

# strip canonical or per-role-werk root prefix -> repo-relative path
rel_of() { echo "$1" | sed -E 's|.*/(chorus(-werk/[^/]+)?)/||'; }

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "enrichment-write-fileInDomain: Fuseki not reachable" >&2
  "$CHORUS_LOG" enrichment.fileInDomain.failed "$ROLE" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')

query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f ?p WHERE { GRAPH <'"$HYDRATION_GRAPH"'> { ?f a chorus:File ; chorus:filePath ?p } }'
resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$query" "$FUSEKI_BASE/query" 2>/dev/null)

files=0; edges=0; failures=0; batch_body=""

flush_batch() {
  [ -z "$batch_body" ] && return 0
  local rc; rc=$(post_update "$batch_body")
  [ "$rc" != "200" ] && [ "$rc" != "204" ] && failures=$((failures + 1))
  batch_body=""
}

while IFS=$'\t' read -r uri filepath; do
  [ -z "$uri" ] && continue
  case "$filepath" in *"/chorus-werk/"*) continue ;; esac   # canonical only
  rel=$(rel_of "$filepath")

  dom=""; owner=""
  for entry in "${BELONGS_MAP[@]}"; do
    p="${entry%%|*}"; rest="${entry#*|}"
    if [ "$rel" = "$p" ]; then dom="${rest%%|*}"; owner="${rest#*|}"; break; fi
  done
  [ -z "$dom" ] && continue

  batch_body="${batch_body}DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileInDomain> ?_d } } ;
DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileHasOwner> ?_o } } ;
INSERT DATA { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileInDomain> <${CHORUS_NS}${dom}> ; <${CHORUS_NS}fileHasOwner> <${CHORUS_NS}${owner}> } } ;
"
  files=$((files + 1)); edges=$((edges + 1))
done < <(echo "$resp" | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    for b in d['results']['bindings']:
        print(b.get('f',{}).get('value',''), b.get('p',{}).get('value',''), sep='\t')
except Exception:
    pass
")

flush_batch

end_ts=$(python3 -c 'import time; print(int(time.time()*1000))')
duration_ms=$((end_ts - start_ts))

"$CHORUS_LOG" enrichment.fileInDomain.written "$ROLE" \
  files="$files" edges="$edges" duration_ms="$duration_ms" failures="$failures" 2>/dev/null || true

echo "fileInDomain (by function): ${files} files tagged (${failures} batch failure(s), ${duration_ms}ms)"
exit $(( failures > 0 ? 1 : 0 ))
