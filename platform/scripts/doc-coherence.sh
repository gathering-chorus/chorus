#!/usr/bin/env bash
# doc-coherence.sh — reads inventory TSV, reports drift categories (#2461).
#
# Checks:
#   1. Content-hash duplicates — same content at multiple paths (real forks)
#   2. Basename duplicates — same filename, different content (catalog ambiguity)
#   3. Broken hrefs — catalog hrefs returning non-200 (live probe against localhost:3340 chorus-api,
#      falling back to localhost:3000 Gathering app for hrefs served from gathering/public/)
#
# Output: markdown report written to $REPORT.
# Env:
#   CHORUS_REPO  (default: /Users/jeffbridwell/CascadeProjects/chorus)
#   INVENTORY    (default: $CHORUS_REPO/knowledge/doc-inventory.tsv)
#   REPORT       (default: $CHORUS_REPO/knowledge/doc-coherence.md)
#   SKIP_HREF_PROBE=1  skip live catalog probe (for offline tests)
set -uo pipefail

CHORUS_REPO="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
INVENTORY="${INVENTORY:-${CHORUS_REPO}/knowledge/doc-inventory.tsv}"
REPORT="${REPORT:-${CHORUS_REPO}/knowledge/doc-coherence.md}"
SKIP_HREF_PROBE="${SKIP_HREF_PROBE:-0}"

[ -f "$INVENTORY" ] || { echo "coherence: $INVENTORY not found — run doc-inventory.sh first" >&2; exit 1; }

# Group content-dupes (col 8 = hash)
CONTENT_DUPES=$(awk -F'\t' '$8 != "" {h[$8] = h[$8]","$2; c[$8]++} END {for (k in c) if (c[k] > 1) print k"\t"c[k]"\t"substr(h[k],2)}' "$INVENTORY" | sort -t$'\t' -k2 -rn)
CONTENT_DUP_GROUPS=$(echo "$CONTENT_DUPES" | grep -c . | tr -d ' ')
[ "$CONTENT_DUPES" = "" ] && CONTENT_DUP_GROUPS=0

# Group basename-dupes using flat-key awk (portable BWK)
BASENAME_DUPES=$(awk -F'\t' '
  $8 != "" {
    bn=$2; sub(/.*\//,"",bn)
    bn_count[bn]++
    bn_paths[bn] = bn_paths[bn] " ; " $2 " (" $8 ")"
    hashkey = bn "|" $8
    if (!seen_hash[hashkey]) {
      seen_hash[hashkey] = 1
      bn_distinct_hashes[bn]++
    }
  }
  END {
    for (bn in bn_count) {
      if (bn_count[bn] > 1 && bn_distinct_hashes[bn] > 1) {
        sub(/^ ; /, "", bn_paths[bn])
        print bn"\t"bn_count[bn]"\t"bn_distinct_hashes[bn]"\t"bn_paths[bn]
      }
    }
  }
' "$INVENTORY" | sort -t$'\t' -k2 -rn)
BASENAME_DUP_GROUPS=$(echo "$BASENAME_DUPES" | grep -c . | tr -d ' ')
[ "$BASENAME_DUPES" = "" ] && BASENAME_DUP_GROUPS=0

# Broken hrefs (live probe, skip if SKIP_HREF_PROBE=1)
#
# #2994: CHORUS_API_HOST env override lets pre-commit point the probe at a
# side-port chorus-api (built from werk dist) instead of canonical :3340.
# Closes the chicken-egg where a card adds new static-mount routes — the
# routes can't be probed against canonical because canonical is running the
# pre-card dist. Default unchanged: localhost:3340 + fallback localhost:3000.
BROKEN_HREFS=""
BROKEN_COUNT=0
PROBE_HOST="${CHORUS_API_HOST:-localhost:3340}"
FALLBACK_HOST="${CHORUS_FALLBACK_HOST:-localhost:3000}"
# #2994: pre-/acp side-port auto-detect. If CHORUS_API_HOST is unset and a
# side-port chorus-api is up on 3345 (the pre-/acp verify port — Wren and Silas
# both use this), prefer it. This lets the ratchet see the werk dist's routes
# before the deploy lands. After /acp the side-port goes idle and the next
# coherence run falls through to canonical :3340.
if [ -z "${CHORUS_API_HOST:-}" ]; then
  if curl -s -o /dev/null --max-time 1 -w "%{http_code}" "http://localhost:3345/api/chorus/health" 2>/dev/null | grep -q "^200$"; then
    PROBE_HOST="localhost:3345"
  fi
fi
if [ "$SKIP_HREF_PROBE" != "1" ]; then
  # Pull catalog hrefs from chorus-api (#2445 wave 3 — was :3000 gathering).
  CATALOG_JSON=$(curl -s --max-time 5 "http://$PROBE_HOST/api/doc-catalog" 2>/dev/null)
  if [ -n "$CATALOG_JSON" ]; then
    # Extract hrefs, probe in parallel (xargs), collect non-2xx
    HREFS=$(echo "$CATALOG_JSON" | python3 -c "import json,sys; d=json.load(sys.stdin); print('\n'.join(doc['href'] for g in d['groups'] for doc in g['docs']))" 2>/dev/null)
    if [ -n "$HREFS" ]; then
      BROKEN_HREFS=$(echo "$HREFS" | PROBE_HOST="$PROBE_HOST" FALLBACK_HOST="$FALLBACK_HOST" xargs -I{} -P 16 -n 1 sh -c '
        # Catalog hrefs are served by chorus-api ($PROBE_HOST) for chorus-side
        # content and Gathering app ($FALLBACK_HOST) for gathering/public/
        # content. Probe primary first; on 404 fall through to fallback before
        # reporting broken. 200 + redirect codes (301/302/308) all count as
        # fine — 308 is permanent redirect used by ADR migration paths.
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 3 "http://$PROBE_HOST$1" 2>/dev/null)
        case "$code" in
          200|301|302|308) : ;;
          404)
            code2=$(curl -s -o /dev/null -w "%{http_code}" --max-time 3 "http://$FALLBACK_HOST$1" 2>/dev/null)
            case "$code2" in 200|301|302|308) : ;; *) echo "$code2 $1" ;; esac
            ;;
          *) echo "$code $1" ;;
        esac
      ' _ {})
      BROKEN_COUNT=$(echo "$BROKEN_HREFS" | grep -c . | tr -d ' ')
      [ "$BROKEN_HREFS" = "" ] && BROKEN_COUNT=0
    fi
  fi
fi

# Write report
{
  echo "# Doc coherence report — $(date '+%Y-%m-%d %H:%M')"
  echo ""
  echo "content-dup-groups: $CONTENT_DUP_GROUPS"
  echo "basename-dup-groups: $BASENAME_DUP_GROUPS"
  echo "broken-hrefs: $BROKEN_COUNT"
  echo ""
  echo "## Content-hash duplicates"
  echo ""
  if [ "$CONTENT_DUP_GROUPS" -eq 0 ]; then
    echo "_None._"
  else
    echo "Same content at multiple paths (real forks — #2461 coherence resolves per-dup)."
    echo ""
    echo "$CONTENT_DUPES" | while IFS=$'\t' read -r hash count paths; do
      echo "- hash \`$hash\` ($count copies):"
      echo "$paths" | tr ',' '\n' | sed 's|^|  - |'
    done
  fi
  echo ""
  echo "## Basename duplicates"
  echo ""
  if [ "$BASENAME_DUP_GROUPS" -eq 0 ]; then
    echo "_None._"
  else
    echo "Same filename at multiple paths with different content. Catalog \`seenHref\`/\`seenTitle\` dedup picks one nondeterministically."
    echo ""
    echo "$BASENAME_DUPES" | while IFS=$'\t' read -r bn count n_hashes paths; do
      echo "- \`$bn\` ($count paths, $n_hashes distinct hashes):"
      echo "$paths" | tr ';' '\n' | sed 's/^ *//; s/^/  - /'
    done
  fi
  echo ""
  echo "## Broken hrefs"
  echo ""
  if [ "$SKIP_HREF_PROBE" = "1" ]; then
    echo "_Skipped (SKIP_HREF_PROBE=1)._"
  elif [ "$BROKEN_COUNT" -eq 0 ]; then
    echo "_None. All catalog hrefs return 200 or 302 (auth-gated)._"
  else
    echo "Catalog hrefs returning non-2xx / non-302:"
    echo ""
    echo '```'
    echo "$BROKEN_HREFS"
    echo '```'
  fi
} > "$REPORT"

echo "→ $REPORT" >&2
echo "  content-dup-groups:  $CONTENT_DUP_GROUPS" >&2
echo "  basename-dup-groups: $BASENAME_DUP_GROUPS" >&2
echo "  broken-hrefs:        $BROKEN_COUNT" >&2
