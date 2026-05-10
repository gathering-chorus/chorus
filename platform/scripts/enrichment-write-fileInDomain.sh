#!/usr/bin/env bash
# enrichment-write-fileInDomain.sh — #2844: enrichment-side writer that maps
# chorus:File instances (hydrated by #2827's crawler) to their Athena
# subdomain via chorus:fileInDomain, and to their owner role via
# chorus:fileHasOwner.
#
# Two derivation rules:
#   1. Path → subdomain via the SUBDOMAIN_MAP table below (longest-prefix
#      match). Files matching no entry go in no_match_count for visibility.
#   2. Path → owner: roles/<role>/ heuristic wins; otherwise fall back
#      to the matched subdomain's chorus:ownedBy (queried from graph).
#
# Idempotent per-record: DELETE-WHERE per (uri, predicate) then INSERT.
# Honors #2827's chorus:writeOwner=chorus:enrichment contract.
#
# Spine event:
#   enrichment.fileInDomain.written {count, duration_ms, no_match_count, failures}
#
# Out of scope:
#   - chorus:Test class predicates (chorus:hasLayer / chorus:bindsScenario) — #2818
#   - frontmatter parsing — #2818 (path-based mapping only here)
#   - cross-machine — won't-do per #2792

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-$FUSEKI_BASE/update}"
HYDRATION_GRAPH="${HYDRATION_GRAPH:-urn:chorus:instances}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# Path → subdomain mapping. Longest-prefix-first because the script will
# iterate top-to-bottom and bind on first match. Each entry is
#   <regex>|<subdomain-id>
# matched against path RELATIVE TO CHORUS_ROOT.
# Per Wren's brief 2026-05-09 + Jeff's call: tighten to STRUCTURALLY
# UNAMBIGUOUS mappings only. A test for cards CLI is BOTH tests-domain
# AND cards-service — that multi-valued reality belongs in a follow-on
# (real classifier with content + frontmatter + multi-valued predicates).
# Until then: only map paths where the file's domain is unambiguous.
#
# Drops from the prior map:
#   - ^proving/ (over-broad; not all of proving is tests)
#   - ^platform/api/src/ catch-all → cards-service (lies about MCP, traces)
#   - ^platform/scripts/chorus- catch-all → spine-service (over-broad)
#   - ^platform/scripts/{validate,crawler,enrichment}-* → tests-domain
#     (these aren't tests, they're hydration/validation/enrichment scripts)
#   - ^platform/services/chorus-hooks → security-domain (hooks are not
#     security in a rigorous sense — Jeff)
#   - ^platform/services/chorus-inject → security-domain (same)
#   - ^platform/pulse → spine-service (pulse is its own concern)
#   - ^designing/ and ^directing/ → chorus-domain (way over-broad)
#   - ^directing/products/ → cards-service (over-bucketing tests etc.)
#   - ^roles/ → roles-domain (Wren: 2622 files = smoking gun)
SUBDOMAIN_MAP=(
  '^proving/scripts/tests/|tests-domain'
  '^proving/domains/alerts/|alerts-monitors-domain'
  '^platform/api/src/sparql/|athena-domain'
  '^platform/api/src/observability/|observability-domain'
  '^platform/api/src/cards/|cards-service'
  '^platform/api/src/mcp/|spine-service'
  '^platform/scripts/git-|version-control-domain'
  '^platform/scripts/chorus-werk|version-control-domain'
  '^platform/scripts/gate-|gates-service'
  '^platform/scripts/cards|cards-service'
  '^platform/scripts/chorus-log|spine-service'
  '^platform/scripts/smoke-|tests-domain'
  '^platform/launchd/|deploys-domain'
  '^.github/workflows/|pipelines-domain'
  '^skills/|skills-service'
  '^building/|build-domain'
  '^knowledge/|knowledge-domain'
  '^docs/|knowledge-domain'
  '^dashboards/|observability-domain'
  '^config/|infrastructure-domain'
)

# Graph-declared chorus:hasPathPattern wins; SUBDOMAIN_MAP is the fallback.
# Patterns load once at startup into a temp file, sorted longest-first
# (longest-prefix match = most specific subdomain wins).
GRAPH_PATTERNS_FILE=$(mktemp -t enrich-patterns.XXXXXX)

load_graph_patterns() {
  local query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?sd ?pattern WHERE {
  GRAPH <urn:chorus:instances> {
    ?sd a chorus:SubDomain ; chorus:hasPathPattern ?pattern .
  }
}'
  curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$query" \
    "$FUSEKI_BASE/query" 2>/dev/null \
  | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    for b in d['results']['bindings']:
        sd_uri = b.get('sd',{}).get('value','')
        sd = sd_uri.split('#')[-1].split('/')[-1]
        pat = b.get('pattern',{}).get('value','')
        if pat: print(f'{pat}|{sd}')
except Exception:
    pass
" | awk '{print length, $0}' | sort -k1 -nr | cut -d' ' -f2- > "$GRAPH_PATTERNS_FILE"
}

path_to_subdomain() {
  local rel="$1"
  # Prefer graph-declared patterns
  if [ -s "$GRAPH_PATTERNS_FILE" ]; then
    local entry
    while IFS= read -r entry; do
      [ -z "$entry" ] && continue
      local pattern="${entry%%|*}"
      local subdomain="${entry##*|}"
      if echo "$rel" | grep -qE "$pattern"; then
        echo "$subdomain"
        return 0
      fi
    done < "$GRAPH_PATTERNS_FILE"
  fi
  # Fallback to script-side SUBDOMAIN_MAP (covers anything graph didn't declare)
  for entry in "${SUBDOMAIN_MAP[@]}"; do
    local pattern="${entry%%|*}"
    local subdomain="${entry##*|}"
    if echo "$rel" | grep -qE "$pattern"; then
      echo "$subdomain"
      return 0
    fi
  done
  echo ""
}

# Path-based owner heuristic only. Subdomain-owner fallback (would require
# associative-array cache of the chorus:ownedBy SPARQL query) deferred to
# follow-on per bash 3.2 portability constraint on macOS default shell.
path_to_owner() {
  local rel="$1"
  if echo "$rel" | grep -qE '^roles/(kade|wren|silas|jeff)/'; then
    echo "$rel" | sed -E 's|^roles/([^/]+)/.*|\1|'
    return 0
  fi
  echo ""
}

post_update() {
  curl -s -o /tmp/enrichment-resp.txt -w '%{http_code}' \
    -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "$1" \
    "$FUSEKI_UPDATE" 2>/dev/null || echo "000"
}

# --- main ---

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "enrichment-write-fileInDomain: Fuseki not reachable" >&2
  "$CHORUS_LOG" enrichment.fileInDomain.failed "$ROLE" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

load_graph_patterns

start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')

query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f ?p WHERE {
  GRAPH <'"$HYDRATION_GRAPH"'> {
    ?f a chorus:File ; chorus:filePath ?p .
  }
}'
resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$query" \
  "$FUSEKI_BASE/query" 2>/dev/null)

count=0
no_match=0
batch_body=""
batch_size=0
failures=0

flush_batch() {
  [ -z "$batch_body" ] && return 0
  local rc
  rc=$(post_update "$batch_body")
  if [ "$rc" != "200" ] && [ "$rc" != "204" ]; then
    failures=$((failures + 1))
  fi
  batch_body=""
  batch_size=0
}

while IFS=$'\t' read -r uri filepath; do
  [ -z "$uri" ] && continue
  # Strip any known chorus tree root prefix (canonical or per-role werk).
  # Files in the graph may have hydrated from werk in one run and canonical
  # in another, so a single CHORUS_ROOT strip isn't enough.
  rel=$(echo "$filepath" | sed -E 's|.*/(chorus(-werk/[^/]+)?)/||')
  subdomain=$(path_to_subdomain "$rel")
  if [ -z "$subdomain" ]; then
    no_match=$((no_match + 1))
    continue
  fi
  owner=$(path_to_owner "$rel")

  batch_body="${batch_body}DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileInDomain> ?_d } } ;
"
  batch_body="${batch_body}INSERT DATA { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileInDomain> <${CHORUS_NS}${subdomain}> } } ;
"
  if [ -n "$owner" ]; then
    batch_body="${batch_body}DELETE WHERE { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileHasOwner> ?_o } } ;
"
    batch_body="${batch_body}INSERT DATA { GRAPH <${HYDRATION_GRAPH}> { <${uri}> <${CHORUS_NS}fileHasOwner> <${CHORUS_NS}${owner}> } } ;
"
  fi

  count=$((count + 1))
  batch_size=$((batch_size + 1))
  if [ "$batch_size" -ge 100 ]; then
    flush_batch
  fi
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
  count="$count" no_match_count="$no_match" duration_ms="$duration_ms" failures="$failures" 2>/dev/null || true

echo "Enrichment: wrote ${count} chorus:fileInDomain triples (${no_match} unmatched, ${failures} batch failure(s), ${duration_ms}ms)"

exit $(( failures > 0 ? 1 : 0 ))
