#!/usr/bin/env bash
# graph-lint.sh — RDF graph coherence scanner
# Runs SPARQL queries against Fuseki to check graph health.
# Usage: graph-lint.sh [--fix] [--quiet]
#
# Card: #508

set -u

FUSEKI_QUERY="http://localhost:3030/pods/query"
FUSEKI_UPDATE="http://localhost:3030/pods/update"
ENV_FILE="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env"
MANIFEST_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/data/harvest/manifests"
ONTOLOGY="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/src/ontology/jb-ontology.ttl"

FUSEKI_PW=""
if [ -f "$ENV_FILE" ]; then
  FUSEKI_PW=$(grep -m1 '^FUSEKI_ADMIN_PASSWORD=' "$ENV_FILE" | cut -d= -f2 | tr -d '"' 2>/dev/null || true)
fi

FIX=false
QUIET=false
for arg in "$@"; do
  case "$arg" in
    --fix) FIX=true ;;
    --quiet) QUIET=true ;;
  esac
done

PASS=0
WARN=0
FAIL=0

pass() { PASS=$((PASS + 1)); $QUIET || echo "  ✓ $1"; }
warn() { WARN=$((WARN + 1)); echo "  △ $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  ✗ $1"; }

sparql() {
  curl -s --max-time 30 "$FUSEKI_QUERY" \
    -H 'Content-Type: application/sparql-query' \
    -H 'Accept: application/sparql-results+json' \
    -d "$1"
}

echo "Graph Coherence Lint"
echo "===================="
echo ""

# ── 1. URI Scheme Consistency ──────────────────────────────────
echo "1. URI scheme consistency"

OLD_SCHEME=$(sparql 'SELECT (COUNT(DISTINCT ?g) AS ?c) WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/")) }' \
  | jq -r '.results.bindings[0].c.value')

if [ "$OLD_SCHEME" = "0" ]; then
  pass "All graphs use http://localhost:3000/ scheme"
else
  fail "$OLD_SCHEME graphs still using https://jeffbridwell.com/ scheme"
  if ! $QUIET; then
    sparql 'SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/")) } ORDER BY ?g' \
      | jq -r '.results.bindings[].g.value' | while IFS= read -r g; do
      echo "      $g"
    done
  fi
fi

# ── 2. Known Domain Prefixes ──────────────────────────────────
echo "2. Graph domain coverage"

KNOWN_PREFIXES=(
  "http://localhost:3000/pods/jeff/music/"
  "http://localhost:3000/pods/jeff/photos/"
  "http://localhost:3000/pods/jeff/media/"
  "http://localhost:3000/pods/jeff/stories/"
  "http://localhost:3000/pods/jeff/notes/"
  "http://localhost:3000/pods/jeff/socialposts/"
  "http://localhost:3000/pods/jeff/blog/"
  "http://localhost:3000/pods/jeff/ontology/"
)

# Build FILTER for known prefixes
FILTER_PARTS=""
for p in "${KNOWN_PREFIXES[@]}"; do
  [ -n "$FILTER_PARTS" ] && FILTER_PARTS="$FILTER_PARTS && "
  FILTER_PARTS="${FILTER_PARTS}!STRSTARTS(STR(?g), \"$p\")"
done

ORPHANS=$(sparql "SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } FILTER($FILTER_PARTS) } ORDER BY ?g" \
  | jq -r '.results.bindings[].g.value')

if [ -z "$ORPHANS" ]; then
  pass "All graphs belong to known domain prefixes"
else
  ORPHAN_COUNT=$(echo "$ORPHANS" | wc -l | tr -d ' ')
  warn "$ORPHAN_COUNT graphs outside known domain prefixes"
  if ! $QUIET; then
    echo "$ORPHANS" | while IFS= read -r g; do
      echo "      $g"
    done
  fi
fi

# ── 3. Predicate Coverage ─────────────────────────────────────
echo "3. Predicate coverage per domain"

DOMAINS=("music" "photos" "media" "stories" "notes")
DOMAIN_PREFIXES=(
  "http://localhost:3000/pods/jeff/music/"
  "http://localhost:3000/pods/jeff/photos/"
  "http://localhost:3000/pods/jeff/media/"
  "http://localhost:3000/pods/jeff/stories/"
  "http://localhost:3000/pods/jeff/notes/"
)

REQUIRED_PREDS=(
  "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
)

RECOMMENDED_PREDS=(
  "http://purl.org/dc/terms/created"
  "http://purl.org/dc/terms/title"
)

for i in "${!DOMAINS[@]}"; do
  domain="${DOMAINS[$i]}"
  prefix="${DOMAIN_PREFIXES[$i]}"

  PREDS=$(sparql "SELECT DISTINCT ?p WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), \"$prefix\")) }" \
    | jq -r '.results.bindings[].p.value' 2>/dev/null)

  if [ -z "$PREDS" ]; then
    warn "$domain: no graphs found at $prefix"
    continue
  fi

  for rp in "${REQUIRED_PREDS[@]}"; do
    if echo "$PREDS" | grep -q "^${rp}$"; then
      pass "$domain: has rdf:type"
    else
      fail "$domain: missing rdf:type"
    fi
  done

  for rp in "${RECOMMENDED_PREDS[@]}"; do
    pred_short=$(echo "$rp" | sed 's|.*/||')
    if echo "$PREDS" | grep -q "^${rp}$"; then
      pass "$domain: has dcterms:$pred_short"
    else
      warn "$domain: missing dcterms:$pred_short"
    fi
  done
done

# ── 4. Type Consistency ───────────────────────────────────────
echo "4. Type consistency against ontology"

# Extract known types from ontology
KNOWN_TYPES=$(grep '^jb:' "$ONTOLOGY" | grep -E 'a\s+(owl|rdfs):Class' | sed 's/[[:space:]]*a .*//' | sed 's/^jb:/https:\/\/jeffbridwell.com\/ontology#/')

# Get all types in Fuseki under jb: namespace
FUSEKI_TYPES=$(sparql 'SELECT DISTINCT ?t WHERE { GRAPH ?g { ?s a ?t } FILTER(STRSTARTS(STR(?t), "https://jeffbridwell.com/ontology#")) }' \
  | jq -r '.results.bindings[].t.value' 2>/dev/null | sort)

UNKNOWN_TYPES=""
while IFS= read -r t; do
  [ -z "$t" ] && continue
  if ! echo "$KNOWN_TYPES" | grep -q "^${t}$"; then
    UNKNOWN_TYPES="${UNKNOWN_TYPES}${t}\n"
  fi
done <<< "$FUSEKI_TYPES"

if [ -z "$UNKNOWN_TYPES" ]; then
  pass "All rdf:type values match jb-ontology.ttl classes"
else
  UNKNOWN_COUNT=$(echo -e "$UNKNOWN_TYPES" | grep -c . || true)
  warn "$UNKNOWN_COUNT types in Fuseki not declared in ontology"
  if ! $QUIET; then
    echo -e "$UNKNOWN_TYPES" | while IFS= read -r t; do
      [ -z "$t" ] && continue
      echo "      $t"
    done
  fi
fi

# ── 5. Property Coverage ─────────────────────────────────────
echo "5. Property coverage against ontology"

# Extract known properties from ontology
KNOWN_PROPS=$(grep '^jb:' "$ONTOLOGY" | grep -E 'a\s+owl:(Datatype|Object|Annotation)Property' | sed 's/[[:space:]]*a .*//' | sed 's/^jb:/https:\/\/jeffbridwell.com\/ontology#/')

# Get all jb: properties in Fuseki
FUSEKI_PROPS=$(sparql 'SELECT DISTINCT ?p WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?p), "https://jeffbridwell.com/ontology#")) }' \
  | jq -r '.results.bindings[].p.value' 2>/dev/null | sort)

UNKNOWN_PROPS=""
UNKNOWN_PROP_COUNT=0
while IFS= read -r p; do
  [ -z "$p" ] && continue
  if ! echo "$KNOWN_PROPS" | grep -q "^${p}$"; then
    UNKNOWN_PROPS="${UNKNOWN_PROPS}${p}\n"
    UNKNOWN_PROP_COUNT=$((UNKNOWN_PROP_COUNT + 1))
  fi
done <<< "$FUSEKI_PROPS"

if [ "$UNKNOWN_PROP_COUNT" -eq 0 ]; then
  pass "All jb: properties match jb-ontology.ttl declarations"
else
  warn "$UNKNOWN_PROP_COUNT properties in Fuseki not declared in ontology"
  if ! $QUIET; then
    echo -e "$UNKNOWN_PROPS" | while IFS= read -r p; do
      [ -z "$p" ] && continue
      echo "      $p"
    done
  fi
fi

# ── 6. Manifest Drift ────────────────────────────────────────
echo "6. Manifest vs Fuseki count drift"

MANIFEST_DOMAINS=("music" "photos" "sexuality" "stories" "notes" "wordpress")
MANIFEST_PREFIXES=(
  "http://localhost:3000/pods/jeff/music/"
  "http://localhost:3000/pods/jeff/photos/"
  "http://localhost:3000/pods/jeff/media/"
  "http://localhost:3000/pods/jeff/stories/"
  "http://localhost:3000/pods/jeff/notes/"
  "http://localhost:3000/pods/jeff/blog/"
)

for i in "${!MANIFEST_DOMAINS[@]}"; do
  domain="${MANIFEST_DOMAINS[$i]}"
  prefix="${MANIFEST_PREFIXES[$i]}"
  manifest="$MANIFEST_DIR/${domain}.json"

  if [ ! -f "$manifest" ]; then
    warn "$domain: no manifest file"
    continue
  fi

  MANIFEST_COUNT=$(jq -r '.stages.load.fuseki_count // empty' "$manifest" 2>/dev/null)
  if [ -z "$MANIFEST_COUNT" ]; then
    warn "$domain: manifest missing fuseki_count"
    continue
  fi

  ACTUAL=$(sparql "SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH ?g { ?s a ?t } FILTER(STRSTARTS(STR(?g), \"$prefix\")) }" \
    | jq -r '.results.bindings[0].c.value' 2>/dev/null)

  if [ -z "$ACTUAL" ] || [ "$ACTUAL" = "null" ]; then
    warn "$domain: Fuseki query failed"
    continue
  fi

  DIFF=$((ACTUAL - MANIFEST_COUNT))
  ABS_DIFF=${DIFF#-}
  if [ "$ABS_DIFF" -eq 0 ]; then
    pass "$domain: manifest matches Fuseki ($ACTUAL)"
  elif [ "$ABS_DIFF" -lt 100 ]; then
    warn "$domain: drift $DIFF (manifest: $MANIFEST_COUNT, actual: $ACTUAL)"
  else
    fail "$domain: drift $DIFF (manifest: $MANIFEST_COUNT, actual: $ACTUAL)"
  fi
done

# ── 7. Manifest Gaps Freshness ────────────────────────────────
echo "7. Manifest staleness"

for domain in "${MANIFEST_DOMAINS[@]}"; do
  manifest="$MANIFEST_DIR/${domain}.json"
  [ ! -f "$manifest" ] && continue

  UPDATED=$(jq -r '.updated // empty' "$manifest" 2>/dev/null)
  if [ -z "$UPDATED" ]; then
    warn "$domain: no updated timestamp"
    continue
  fi

  # Days since update (macOS date)
  UPDATED_EPOCH=$(date -j -f '%Y-%m-%dT%H:%M:%SZ' "$UPDATED" '+%s' 2>/dev/null || date -j -f '%Y-%m-%dT%H:%M:%S' "${UPDATED%Z}" '+%s' 2>/dev/null || echo "0")
  NOW_EPOCH=$(date '+%s')
  DAYS_AGO=$(( (NOW_EPOCH - UPDATED_EPOCH) / 86400 ))

  if [ "$DAYS_AGO" -le 7 ]; then
    pass "$domain: updated ${DAYS_AGO}d ago"
  elif [ "$DAYS_AGO" -le 30 ]; then
    warn "$domain: updated ${DAYS_AGO}d ago"
  else
    fail "$domain: updated ${DAYS_AGO}d ago — stale"
  fi
done

# ── Summary ───────────────────────────────────────────────────
echo ""
echo "Summary: $PASS pass, $WARN warn, $FAIL fail"

# Emit spine event
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log.sh"
if [ -x "$CHORUS_LOG" ]; then
  local_status="ok"
  [ "$WARN" -gt 0 ] && local_status="warn"
  [ "$FAIL" -gt 0 ] && local_status="fail"
  "$CHORUS_LOG" ops.graphlint.completed silas "pass=$PASS" "warn=$WARN" "fail=$FAIL" "status=$local_status" 2>/dev/null || true
fi

if [ "$FAIL" -gt 0 ]; then
  exit 2
elif [ "$WARN" -gt 0 ]; then
  exit 1
else
  exit 0
fi
