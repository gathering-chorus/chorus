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
FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
CHORUS_NS="https://jeffbridwell.com/chorus#"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

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

  # Walk filesystem with exclusions; cap large batches at 200 files per UPDATE
  # to keep payload sizes reasonable for Fuseki.
  local count=0 batch=0 batch_triples="" failures=0
  local DELETE_PREFIX="DELETE WHERE { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { ?s a <${CHORUS_NS}File> ; <${CHORUS_NS}filePath> ?p } } ;"
  # Issue a single bulk DELETE before any inserts (idempotent replace-on-write):
  # next run wipes the chorus:File class instances, then re-inserts current state.
  local rc
  rc=$(post_update "$DELETE_PREFIX")
  if [ "$rc" != "200" ] && [ "$rc" != "204" ]; then
    "$CHORUS_LOG" crawler.graph.failed "$ROLE" class="chorus:File" reason="delete-prefix-fail-http-$rc" 2>/dev/null || true
    return 1
  fi

  # Use process substitution + null-delimited paths
  while IFS= read -r -d '' f; do
    batch_triples="$batch_triples$(file_triples "$f")
"
    count=$((count + 1))
    batch=$((batch + 1))
    if [ "$batch" -ge 200 ]; then
      rc=$(post_update "INSERT DATA { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $batch_triples } }")
      if [ "$rc" != "200" ] && [ "$rc" != "204" ]; then
        failures=$((failures + 1))
      fi
      batch_triples=""
      batch=0
    fi
  done < <(eval "find \"$CHORUS_ROOT\" $FIND_PRUNE -type f -print0" 2>/dev/null)

  # Final batch
  if [ "$batch" -gt 0 ]; then
    rc=$(post_update "INSERT DATA { GRAPH <${HYDRATION_GRAPH:-urn:chorus:instances}> { $batch_triples } }")
    if [ "$rc" != "200" ] && [ "$rc" != "204" ]; then
      failures=$((failures + 1))
    fi
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

# --- main ---

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "crawler-hydrate-graph: Fuseki not reachable at localhost:3030" >&2
  "$CHORUS_LOG" crawler.graph.failed "$ROLE" class="*" reason="fuseki-unreachable" 2>/dev/null || true
  exit 1
fi

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

exit $(( errors > 0 ? 1 : 0 ))
