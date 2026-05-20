#!/usr/bin/env bash
# enrichment-write-fileDependsOn.sh — #3017: enrichment-side writer that emits
# chorus:fileDependsOn edges for the GLOBAL set of crawler-hydrated chorus:File
# instances, deriving the edge BY FUNCTION (call-site detection), NOT by path.
#
# fileInDomain (#2844) says what a file IS (belongs-to / domain-radius);
# fileDependsOn says what a file USES (depends-on). The inbound set per domain
# is that domain's file-level blast-radius:  blast(D) = { f | f fileDependsOn D }.
#
# DEPENDENCY_MAP pairs an extended-regex over file CONTENT with the domain the
# match implies. A file matching N patterns gets N edges (N:N is normal, #3017).
# Idempotent per matched file: DELETE its fileDependsOn edges, then INSERT the
# detected set. writeOwner=chorus:enrichment (#2827 contract).
#
# Spine event: enrichment.fileDependsOn.written {files, edges, duration_ms, failures}
#
# Validated on chorus:spine (chorus-log emitters). The pass is GLOBAL (iterates
# every hydrated chorus:File); the MAP is the extension point — one line per
# domain surface, no code change.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-$FUSEKI_BASE/update}"
HYDRATION_GRAPH="${HYDRATION_GRAPH:-urn:chorus:instances}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# <extended-regex over file content>|<domain-id>. BY FUNCTION (call-site), not path.
DEPENDENCY_MAP=(
  'chorus-log |chorus_log\(|appendSpine|writeSpine|emitSpine|/chorus-log|chorus_logs_|spine_event|spineEmit|spine\.emit|spine-events\.json|chorus_spine|emit_spine|spine_emit|appendToSpine|writeToSpine|emitSpineEvent|recordSpineEvent|/spine/|/api/spine|spine\.append\(|spine\.write\(|spine\.record\(|spine_tick|chorusLog\(|spine'
)

post_update() {
  curl -s -o /dev/null -w '%{http_code}' \
    -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "$1" "$FUSEKI_UPDATE" 2>/dev/null || echo "000"
}

# resolve a graph filePath (canonical OR per-role werk) to a readable path on
# disk; prefer the canonical tree so content is current.
resolve_path() {
  local fp="$1" rel
  rel=$(echo "$fp" | sed -E 's|.*/(chorus(-werk/[^/]+)?)/||')
  if [ -f "$CHORUS_ROOT/$rel" ]; then echo "$CHORUS_ROOT/$rel"; return; fi
  [ -f "$fp" ] && echo "$fp"
}

# --- main ---

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "enrichment-write-fileDependsOn: Fuseki not reachable" >&2
  "$CHORUS_LOG" enrichment.fileDependsOn.failed "$ROLE" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')

query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f ?p WHERE { GRAPH <'"$HYDRATION_GRAPH"'> { ?f a chorus:File ; chorus:filePath ?p } }'
resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$query" "$FUSEKI_BASE/query" 2>/dev/null)

files=0; edges=0; failures=0; batch_body=""; batch_size=0

flush_batch() {
  [ -z "$batch_body" ] && return 0
  local rc; rc=$(post_update "$batch_body")
  [ "$rc" != "200" ] && [ "$rc" != "204" ] && failures=$((failures + 1))
  batch_body=""; batch_size=0
}

while IFS=$'\t' read -r uri filepath; do
  [ -z "$uri" ] && continue
  # Tag the canonical tree only — skip stale/transient werk instances the
  # crawler hydrated (e.g. the retired chorus-werk/kade/ tree, #2913).
  case "$filepath" in *"/chorus-werk/"*) continue ;; esac
  path=$(resolve_path "$filepath")
  [ -z "$path" ] && continue
  # fileDependsOn is a CODE-dependency edge. A doc mentioning "spine" in prose
  # does not depend on spine — restrict to code/test artifacts.
  case "$path" in
    *.ts|*.tsx|*.js|*.mjs|*.cjs|*.rs|*.sh|*.bash|*.bats|*.py) ;;
    *) continue ;;
  esac

  detected=()
  for entry in "${DEPENDENCY_MAP[@]}"; do
    pat="${entry%|*}"; dom="${entry##*|}"
    if grep -qE "$pat" "$path" 2>/dev/null; then detected+=("$dom"); fi
  done
  [ "${#detected[@]}" -eq 0 ] && continue

  batch_body="${batch_body}DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileDependsOn> ?_d } } ;
"
  ins=""
  for dom in "${detected[@]}"; do
    ins="${ins} <${uri}> <${CHORUS_NS}fileDependsOn> <${CHORUS_NS}${dom}> ."
    edges=$((edges + 1))
  done
  batch_body="${batch_body}INSERT DATA { GRAPH <${HYDRATION_GRAPH}> {${ins} } } ;
"
  files=$((files + 1)); batch_size=$((batch_size + 1))
  [ "$batch_size" -ge 100 ] && flush_batch
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

"$CHORUS_LOG" enrichment.fileDependsOn.written "$ROLE" \
  files="$files" edges="$edges" duration_ms="$duration_ms" failures="$failures" 2>/dev/null || true

echo "fileDependsOn: ${files} files, ${edges} edges (${failures} batch failure(s), ${duration_ms}ms)"
exit $(( failures > 0 ? 1 : 0 ))
