#!/usr/bin/env bash
# enrichment-write-fileDependsOn.sh — #3017/#3021: blast-radius writer.
# fileDependsOn = what a file USES (depends-on). Inbound set per domain =
# that domain's file-level blast-radius:  blast(D) = { f | f fileDependsOn D }.
#
# #3021 rewrite: ONE filesystem ripgrep pass per dependency pattern (not a full
# ~6000-File scan + grep-each-file, which cost 33s), excluding dist/build +
# node_modules + werk and restricting to code/test extensions. Pattern tightened
# to actual emit/use call-sites — drops the soft-token false positives (and the
# dist exclusion drops the dist artifacts) that put precision at ~91%.
#
# Idempotent per (file, domain): DELETE that edge then INSERT it.
# writeOwner=chorus:enrichment. Spine: enrichment.fileDependsOn.written.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-$FUSEKI_BASE/update}"
HYDRATION_GRAPH="${HYDRATION_GRAPH:-urn:chorus:instances}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# <extended-regex of a domain's API surface as USED by a consumer>|<domain>.
# IMPORTANT: the LAST pipe-section is the DOMAIN LABEL (parsed below via
# dom="${entry##*|}"), NOT a regex alternative. So the trailing "spine" names
# the domain — it is not a bare match-everything token. (Don't be fooled into
# feeding the whole entry to ripgrep; the script splits the domain off first.)
#
# Two-sided blast-radius — a file is in spine's radius if it EMITS to spine
# (chorus-log/emitSpine/...) OR CONSUMES it: SpineEntry/SpineEvent types, reads
# chorus.log via spineLogPath/chorusLogPath, parses spine_events, tags
# source:'spine'. Emit-only missed real readers (context-spine.ts, chorus-rcas.ts).
DEPENDENCY_MAP=(
  'chorus-log |chorus_log\(|appendSpine|writeSpine|emitSpine|/chorus-log|chorus_logs_|SpineEntry|SpineEvent|spineLogPath|chorusLogPath|spine_events|source: ?.spine.|spine'
)

post_update() {
  curl -s -o /dev/null -w '%{http_code}' \
    -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "$1" "$FUSEKI_UPDATE" 2>/dev/null || echo "000"
}

# canonical rel-path -> chorus:File URI (targeted; canonical only)
uri_for_rel() {
  local rel="$1"
  local q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f WHERE { GRAPH <'"$HYDRATION_GRAPH"'> { ?f a chorus:File ; chorus:filePath ?p .
  FILTER(STRENDS(STR(?p), "/'"$rel"'") && !CONTAINS(STR(?p), "/chorus-werk/")) } } LIMIT 1'
  curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$q" "$FUSEKI_BASE/query" 2>/dev/null \
    | python3 -c "import json,sys
try:
    b=json.load(sys.stdin)['results']['bindings']; print(b[0]['f']['value'] if b else '')
except Exception: print('')"
}

# --- main ---

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "enrichment-write-fileDependsOn: Fuseki not reachable" >&2
  "$CHORUS_LOG" enrichment.fileDependsOn.failed "$ROLE" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')
files=0; edges=0; failures=0; batch_body=""; batch_size=0

flush_batch() {
  [ -z "$batch_body" ] && return 0
  local rc; rc=$(post_update "$batch_body")
  [ "$rc" != "200" ] && [ "$rc" != "204" ] && failures=$((failures + 1))
  batch_body=""; batch_size=0
}

# rg if available, else grep -r — ONE pass over canonical, excluding dist/build.
have_rg=0; command -v rg >/dev/null 2>&1 && have_rg=1

for entry in "${DEPENDENCY_MAP[@]}"; do
  pat="${entry%|*}"; dom="${entry##*|}"
  if [ "$have_rg" -eq 1 ]; then
    matches=$(rg -l --no-messages \
      -t ts -t js -t rust -t sh -t python \
      -g '!**/dist/**' -g '!**/dist.prev/**' -g '!**/node_modules/**' -g '!**/chorus-werk/**' \
      -e "$pat" "$CHORUS_ROOT" 2>/dev/null)
  else
    matches=$(grep -rlE --include='*.ts' --include='*.js' --include='*.sh' --include='*.rs' --include='*.py' --include='*.bats' \
      --exclude-dir=dist --exclude-dir=dist.prev --exclude-dir=node_modules --exclude-dir=chorus-werk \
      "$pat" "$CHORUS_ROOT" 2>/dev/null)
  fi
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    case "$f" in *"/dist/"*|*"/dist.prev/"*|*"/node_modules/"*|*"/chorus-werk/"*) continue ;; esac
    rel="${f#"$CHORUS_ROOT"/}"
    uri=$(uri_for_rel "$rel")
    [ -z "$uri" ] && continue
    batch_body="${batch_body}DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileDependsOn> <${CHORUS_NS}${dom}> } } ;
INSERT DATA { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileDependsOn> <${CHORUS_NS}${dom}> } } ;
"
    files=$((files + 1)); edges=$((edges + 1)); batch_size=$((batch_size + 1))
    [ "$batch_size" -ge 50 ] && flush_batch
  done <<< "$matches"
done

flush_batch

end_ts=$(python3 -c 'import time; print(int(time.time()*1000))')
duration_ms=$((end_ts - start_ts))

"$CHORUS_LOG" enrichment.fileDependsOn.written "$ROLE" \
  files="$files" edges="$edges" duration_ms="$duration_ms" failures="$failures" 2>/dev/null || true

echo "fileDependsOn: ${files} files, ${edges} edges (${failures} batch failure(s), ${duration_ms}ms)"
exit $(( failures > 0 ? 1 : 0 ))
