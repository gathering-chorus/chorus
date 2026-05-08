#!/usr/bin/env bash
# crawler-hydrate-graph.sh — #2827 §B: hydrate Fuseki with chorus:Hydratable
# instances per the chorus.ttl registry.
#
# Reads chorus.ttl, finds every chorus:Hydratable subclass, builds the
# in-memory map of {class → hydratesFromSource → crawler-owned predicates},
# walks the filesystem per glob, and emits SPARQL UPDATE per class to
# replace-on-write the graph instances.
#
# Idempotent contract: per (class, identifier) tuple, same input → same
# triples regardless of how many times this script runs. Implemented as
# DELETE-WHERE-by-identity then INSERT DATA inside the same UPDATE call,
# so consumers never observe a half-state.
#
# Out of scope here:
#   - Enrichment-owned predicates (chorus:fileInDomain, chorus:fileHasOwner) —
#     written by athena-enrichment-write.ts, not this crawler.
#   - Per-class hydration logic for chorus:Test / chorus:Brief / etc. —
#     #2827 ships chorus:File minimum; other types follow incrementally.
#   - End-of-run reconciliation between SQLite + Fuseki — Section C.
#   - Deletes-reconciliation (chorus:stale flag) — Section D.
#
# Spine events:
#   crawler.graph.hydrated   {class, count, duration_ms}  — one per class
#   crawler.graph.failed     {class, reason}              — Fuseki write fails
#
# Exit codes:
#   0   all hydrated classes wrote cleanly
#   1   one or more class hydration failed (errors logged + spine emitted)

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TTL="${TTL:-$CHORUS_ROOT/roles/silas/ontology/chorus.ttl}"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-$FUSEKI_BASE/update}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# §C: SQLite-side index of hydrated chorus:File records, used for the
# end-of-run reconciliation pass. Path is the primary key; fuseki_status
# tracks whether the corresponding Fuseki write succeeded.
DB_PATH="${HYDRATION_DB:-$HOME/.chorus/index.db}"

ensure_chorus_files_table() {
  sqlite3 "$DB_PATH" "CREATE TABLE IF NOT EXISTS chorus_files (
    path TEXT PRIMARY KEY,
    sha TEXT,
    last_modified TEXT,
    fuseki_status TEXT NOT NULL DEFAULT 'pending',
    last_attempt TEXT
  );" 2>/dev/null
}

# Per-class limits: full chorus tree is too broad for chorus:File="**/*"
# without exclusions. Always exclude .git, node_modules, target, dist.
EXCLUDE_DIRS=(.git node_modules target dist .venv __pycache__)

# Build the find-exclusion fragment once.
FIND_PRUNE=""
for d in "${EXCLUDE_DIRS[@]}"; do
  FIND_PRUNE="$FIND_PRUNE -name $d -prune -o"
done

# Resolve registry: produces lines of the form
#   class<TAB>source-glob<TAB>predicate1,predicate2,...
# where predicate-list is only the chorus:writeOwner=chorus:crawler ones.
resolve_registry() {
  python3 - "$TTL" <<'PYEOF'
import sys, re
ttl_orig = open(sys.argv[1]).read()
# Stripped copy for class/predicate block boundaries (periods in comments).
ttl = re.sub(r'"[^"]*"', '""', ttl_orig, flags=re.DOTALL)

# Subclass map → roots
class_blocks = {}
class_blocks_orig = {}
for m in re.finditer(r'(chorus:\w+)\s+a\s+owl:Class\s*;([^.]+)\.', ttl, re.DOTALL):
    cls, body = m.group(1), m.group(2)
    class_blocks[cls] = body
# Same against original (untouched quoted strings) for source globs etc.
for m in re.finditer(r'(chorus:\w+)\s+a\s+owl:Class\s*;', ttl_orig):
    cls = m.group(1)
    start = m.end()
    # Find next class declaration or end-of-block period at line-start
    nxt = re.search(r'\n(?=chorus:\w+\s+a\s+owl:)', ttl_orig[start:])
    body = ttl_orig[start:start+nxt.start()] if nxt else ttl_orig[start:start+5000]
    class_blocks_orig[cls] = body

# Hydratable roots (transitive subClassOf)
hydratable = {"chorus:Hydratable"}
changed = True
while changed:
    changed = False
    for cls, body in class_blocks.items():
        if cls in hydratable: continue
        if any(p in hydratable for p in re.findall(r'rdfs:subClassOf\s+(chorus:\w+)', body)):
            hydratable.add(cls); changed = True

# Concrete (non-marker) hydratable classes carry hydratesFromSource
sources = {}
for cls in hydratable - {"chorus:Hydratable"}:
    src_match = re.search(r'chorus:hydratesFromSource\s+"([^"]+)"', class_blocks_orig.get(cls, ""))
    if src_match:
        sources[cls] = src_match.group(1)

# Crawler-owned predicates per hydratable class
preds_by_class = {cls: [] for cls in sources}
for m in re.finditer(r'(chorus:\w+)\s+a\s+owl:(?:Object|Datatype)Property\s*;([^.]+)\.', ttl, re.DOTALL):
    pred, body = m.group(1), m.group(2)
    domains = re.findall(r'rdfs:domain\s+(chorus:\w+)', body)
    owners = re.findall(r'chorus:writeOwner\s+(chorus:\w+)', body)
    if "chorus:crawler" not in owners: continue
    for d in domains:
        if d in preds_by_class:
            preds_by_class[d].append(pred)
        elif d == "chorus:Hydratable":
            # Predicate on the marker class applies to all hydratable subclasses
            for cls in preds_by_class: preds_by_class[cls].append(pred)

for cls in sorted(sources):
    print(f"{cls}\t{sources[cls]}\t{','.join(preds_by_class[cls])}")
PYEOF
}

# Compute file's chorus:File triples. Echos one INSERT DATA fragment per file.
# IDENTITY = chorus:filePath ABSOLUTE_PATH (one instance per absolute path).
file_triples() {
  local path="$1"
  local sha lastmod uri
  sha=$(shasum -a 256 "$path" 2>/dev/null | awk '{print $1}')
  lastmod=$(date -u -r "$path" '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo "")
  uri="<urn:chorus:file:$(echo -n "$path" | shasum -a 1 | awk '{print $1}')>"
  cat <<EOF
$uri a <${CHORUS_NS}File> ;
  <${CHORUS_NS}filePath> "$path" ;
  <${CHORUS_NS}fileSha> "$sha" ;
  <${CHORUS_NS}fileLastModified> "$lastmod"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
EOF
}

post_update() {
  local update_body="$1"
  local resp http_code
  resp=$(curl -s -o /tmp/crawler-hydrate-resp.txt -w '%{http_code}' \
    -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "$update_body" \
    "$FUSEKI_UPDATE" 2>/dev/null) || resp="000"
  echo "$resp"
}

hydrate_chorus_file() {
  local glob="$1" preds="$2"
  local start_ts end_ts duration_ms
  start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')

  # Per-URI replace-on-write: for each file the crawler still sees on
  # disk, DELETE the crawler-owned predicates for its URI then INSERT the
  # fresh values. Keeps enrichment-owned predicates (fileInDomain,
  # fileHasOwner) and the §D chorus:stale flag intact across runs. Does
  # NOT wipe class-wide — orphans (instances whose path no longer exists)
  # remain in the graph for §D's deletes-reconciliation pass to flag.
  #
  # Batches use a multi-statement UPDATE: DELETE-WHERE per URI then one
  # INSERT DATA for the batch's triples. 200 URIs per batch.
  local count=0 batch=0 batch_deletes="" batch_triples="" failures=0
  local rc

  # Track which paths are in the current batch so we can mark their
  # SQLite rows wrote/failed atomically with the Fuseki batch outcome.
  local batch_paths=()

  flush_batch() {
    local body="${batch_deletes}INSERT DATA { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $batch_triples } }"
    rc=$(post_update "$body")
    local ts now
    now=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
    if [ "$rc" = "200" ] || [ "$rc" = "204" ]; then
      ts="wrote"
    else
      ts="failed"
      failures=$((failures + 1))
    fi
    # Mark each path's fuseki_status atomically.
    for p in "${batch_paths[@]}"; do
      sqlite3 "$DB_PATH" "UPDATE chorus_files SET fuseki_status='$ts', last_attempt='$now' WHERE path='$(echo "$p" | sed "s/'/''/g")';" 2>/dev/null
    done
    batch_deletes=""
    batch_triples=""
    batch_paths=()
    batch=0
  }

  while IFS= read -r -d '' f; do
    local uri="<urn:chorus:file:$(echo -n "$f" | shasum -a 1 | awk '{print $1}')>"
    # §C: SQLite write FIRST. Per AC: "writes SQLite first, Fuseki second
    # per record". INSERT OR REPLACE is the per-record idempotent.
    local sha_v lastmod_v
    sha_v=$(shasum -a 256 "$f" 2>/dev/null | awk '{print $1}')
    lastmod_v=$(date -u -r "$f" '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo "")
    sqlite3 "$DB_PATH" "INSERT OR REPLACE INTO chorus_files (path, sha, last_modified, fuseki_status, last_attempt)
      VALUES ('$(echo "$f" | sed "s/'/''/g")', '$sha_v', '$lastmod_v', 'pending', NULL);" 2>/dev/null
    batch_paths+=("$f")
    batch_deletes="${batch_deletes}DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}filePath> ?_p } } ;
DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}fileSha> ?_s } } ;
DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}fileLastModified> ?_m } } ;
"
    batch_triples="$batch_triples$(file_triples "$f")
"
    count=$((count + 1))
    batch=$((batch + 1))
    if [ "$batch" -ge 200 ]; then
      flush_batch
    fi
  done < <(eval "find \"$CHORUS_ROOT\" $FIND_PRUNE -type f -print0" 2>/dev/null)

  if [ "$batch" -gt 0 ]; then
    flush_batch
  fi

  end_ts=$(python3 -c 'import time; print(int(time.time()*1000))')
  duration_ms=$((end_ts - start_ts))

  if [ "$failures" -eq 0 ]; then
    "$CHORUS_LOG" crawler.graph.hydrated "$ROLE" class="chorus:File" count="$count" duration_ms="$duration_ms" 2>/dev/null || true
    echo "Hydrated chorus:File: $count files, ${duration_ms}ms"
    return 0
  else
    "$CHORUS_LOG" crawler.graph.failed "$ROLE" class="chorus:File" reason="batch-insert-fail-count-$failures" duration_ms="$duration_ms" 2>/dev/null || true
    echo "FAILED chorus:File: $failures batch(es) failed (count=$count, duration=${duration_ms}ms)" >&2
    return 1
  fi
}

# --- §C: per-record reconciliation across SQLite + Fuseki ---
#
# Walk both stores at end-of-run, find records present in one but not
# the other. Emit hydration.partial.divergent per record. For SQLite-side
# rows with fuseki_status=failed, retry the Fuseki write once with
# backoff; on retry success, flip status to wrote. On retry failure,
# leave the divergent state visible via the spine event.
#
# Read-side contract (documented in crawler-service-design.html):
# consumers see eventual consistency within ~one crawl cycle;
# partial-divergent state is observable via the hydration.partial.divergent
# event. Reconciliation does not block the crawl run — failure on either
# side is recorded, not raised.

reconcile_consistency() {
  # 1. Find SQLite rows with fuseki_status='failed' (Fuseki-side missing).
  local failed_paths
  failed_paths=$(sqlite3 "$DB_PATH" "SELECT path FROM chorus_files WHERE fuseki_status='failed';" 2>/dev/null)
  local divergent=0 recovered=0

  if [ -n "$failed_paths" ]; then
    while IFS= read -r path; do
      [ -z "$path" ] && continue
      "$CHORUS_LOG" hydration.partial.divergent "$ROLE" \
        class="chorus:File" identifier="$path" missing_side="fuseki" recovery_attempted="true" 2>/dev/null || true
      divergent=$((divergent + 1))

      # Retry Fuseki write for this single record. One attempt with brief
      # backoff (250ms); the AC asks for retry-once, no exponential.
      sleep 0.25
      local uri="<urn:chorus:file:$(echo -n "$path" | shasum -a 1 | awk '{print $1}')>"
      local triples
      triples=$(file_triples "$path" 2>/dev/null)
      [ -z "$triples" ] && continue
      local body="DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}filePath> ?_p } } ;
DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}fileSha> ?_s } } ;
DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $uri <${CHORUS_NS}fileLastModified> ?_m } } ;
INSERT DATA { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $triples } }"
      local rc
      rc=$(post_update "$body")
      if [ "$rc" = "200" ] || [ "$rc" = "204" ]; then
        local now
        now=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
        sqlite3 "$DB_PATH" "UPDATE chorus_files SET fuseki_status='wrote', last_attempt='$now' WHERE path='$(echo "$path" | sed "s/'/''/g")';" 2>/dev/null
        recovered=$((recovered + 1))
      fi
    done <<< "$failed_paths"
  fi

  # 2. Find Fuseki-side instances whose path has no SQLite row (graph
  # has it, search index doesn't). Less likely under our flow (SQLite
  # write precedes Fuseki) but the contract requires we detect both
  # directions.
  local query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?p WHERE {
  GRAPH <'"${HYDRATION_GRAPH:-urn:chorus:instances}"'> {
    ?f a chorus:File ; chorus:filePath ?p .
  }
}'
  local resp
  resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$query" \
    "${FUSEKI_BASE:-http://localhost:3030/pods}/query" 2>/dev/null)
  local sqlite_orphans=0
  while IFS= read -r path; do
    [ -z "$path" ] && continue
    local row
    row=$(sqlite3 "$DB_PATH" "SELECT 1 FROM chorus_files WHERE path='$(echo "$path" | sed "s/'/''/g")';" 2>/dev/null)
    if [ -z "$row" ]; then
      "$CHORUS_LOG" hydration.partial.divergent "$ROLE" \
        class="chorus:File" identifier="$path" missing_side="sqlite" recovery_attempted="false" 2>/dev/null || true
      sqlite_orphans=$((sqlite_orphans + 1))
    fi
  done < <(echo "$resp" | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    for b in d['results']['bindings']:
        print(b.get('p',{}).get('value',''))
except Exception:
    pass
")

  echo "Consistency reconciled: $divergent fuseki-failed ($recovered recovered), $sqlite_orphans sqlite-missing"
  return 0
}

# --- §D: deletes reconciliation ---
#
# After hydration, query the graph for every chorus:File instance, check
# whether its filePath still exists on disk. For paths that don't:
#   1. SET chorus:stale true (visibility-not-removal — consumer chooses)
#   2. emit crawler.graph.orphan.detected {class, identifier, last_seen_at,
#      deleted_path}
# For paths that DO exist: clear chorus:stale (file came back).

reconcile_deletes() {
  local query='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f ?p ?stale WHERE {
  GRAPH <'"${HYDRATION_GRAPH:-urn:chorus:instances}"'> {
    ?f a chorus:File ; chorus:filePath ?p .
    OPTIONAL { ?f chorus:stale ?stale }
  }
}'
  local resp
  resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$query" \
    "${FUSEKI_BASE:-http://localhost:3030/pods}/query" 2>/dev/null)

  local orphan_count=0 cleared_count=0 stale_updates=""
  while IFS=$'\t' read -r uri filepath stale_now; do
    [ -z "$uri" ] && continue
    if [ ! -e "$filepath" ]; then
      # Orphan — flag it stale (idempotent: only emit event if not already stale)
      if [ "$stale_now" != "true" ]; then
        local now
        now=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
        "$CHORUS_LOG" crawler.graph.orphan.detected "$ROLE" \
          class="chorus:File" identifier="$filepath" last_seen_at="$now" deleted_path="$filepath" 2>/dev/null || true
        orphan_count=$((orphan_count + 1))
      fi
      stale_updates="${stale_updates}DELETE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { <$uri> chorus:stale ?old } } INSERT { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { <$uri> chorus:stale true } } WHERE { OPTIONAL { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { <$uri> chorus:stale ?old } } } ;
"
    elif [ "$stale_now" = "true" ]; then
      # Was stale, now back — clear the flag
      stale_updates="${stale_updates}DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { <$uri> chorus:stale ?_ } } ;
"
      cleared_count=$((cleared_count + 1))
    fi
  done < <(echo "$resp" | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    for b in d['results']['bindings']:
        uri = b.get('f',{}).get('value','')
        path = b.get('p',{}).get('value','')
        stale = b.get('stale',{}).get('value','')
        print(f'{uri}\t{path}\t{stale}')
except Exception:
    pass
")

  if [ -n "$stale_updates" ]; then
    local update_body="PREFIX chorus: <https://jeffbridwell.com/chorus#>
$stale_updates"
    local rc
    rc=$(post_update "$update_body")
    if [ "$rc" != "200" ] && [ "$rc" != "204" ]; then
      "$CHORUS_LOG" crawler.graph.failed "$ROLE" class="chorus:File" reason="stale-update-fail-http-$rc" 2>/dev/null || true
      return 1
    fi
  fi

  echo "Deletes reconciled: $orphan_count new orphan(s), $cleared_count cleared"
  return 0
}

# --- main ---

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "crawler-hydrate-graph: Fuseki not reachable at localhost:3030" >&2
  "$CHORUS_LOG" crawler.graph.failed "$ROLE" class="*" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

ensure_chorus_files_table

errors=0
while IFS=$'\t' read -r cls glob preds; do
  case "$cls" in
    chorus:File)
      hydrate_chorus_file "$glob" "$preds" || errors=$((errors + 1))
      ;;
    *)
      # Other Hydratable classes follow in #2818 / future cards. For now,
      # emit a no-op event so consumers see the class is registered but
      # no hydrator implementation exists yet.
      "$CHORUS_LOG" crawler.graph.skipped "$ROLE" class="$cls" reason="no-hydrator-implementation-yet" 2>/dev/null || true
      ;;
  esac
done < <(resolve_registry)

# §C: per-record consistency reconciliation across SQLite + Fuseki
reconcile_consistency || errors=$((errors + 1))

# §D: post-hydration deletes reconciliation
reconcile_deletes || errors=$((errors + 1))

exit $(( errors > 0 ? 1 : 0 ))
