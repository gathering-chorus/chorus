#!/bin/bash
# backfill-domain-tags.sh — #1950
# Scan ALL Done cards without domain tags, infer domain from title keywords.
# Usage: backfill-domain-tags.sh [--apply] [--domain seeds]
# Default: dry-run (show proposed tags without applying)
# --domain X: only process cards inferred as domain X

set -uo pipefail

CARDS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards"
APPLY=false
FILTER_DOMAIN=""

for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    --domain) shift; FILTER_DOMAIN="${1:-}" ;;
  esac
done
# Handle --domain as next arg
if [ -n "${2:-}" ] && [ "${1:-}" = "--domain" ]; then
  FILTER_DOMAIN="$2"
fi

# Domain inference from title keywords
infer_domain() {
  local title_lower
  title_lower=$(echo "$1" | tr '[:upper:]' '[:lower:]')

  echo "$title_lower" | grep -qE 'seed|sms|twilio|message.*rout' && { echo "seeds"; return; }
  echo "$title_lower" | grep -qE 'photo|thumbnail|image|takeout|exif|face.*cluster|album import' && { echo "photos"; return; }
  echo "$title_lower" | grep -qE 'music|artist|track|apple.*music|itunes|album.*review|album.*browse' && { echo "music"; return; }
  echo "$title_lower" | grep -qE '\bperson\b|\bpeople\b|face|cluster.*name' && { echo "people"; return; }
  echo "$title_lower" | grep -qE 'book|reading|goodreads' && { echo "books"; return; }
  echo "$title_lower" | grep -qE '\bnote\b|bear.*note' && { echo "notes"; return; }
  echo "$title_lower" | grep -qE 'cook|recipe' && { echo "cooking"; return; }
  echo "$title_lower" | grep -qE 'blog|wordpress|wp-' && { echo "blog"; return; }
  echo "$title_lower" | grep -qE 'docker|launchagent|fuseki|loki|grafana|disk|deploy|nifi|backup|\binfra\b|service.*registry|ssl|nginx|cloudflare|security|auth.*handler|acl|helm|monitoring|prometheus|metric' && { echo "infrastructure"; return; }
  echo "$title_lower" | grep -qE 'clearing|bridge|socket\.io|tile' && { echo "chorus"; return; }
  echo "$title_lower" | grep -qE 'nudge|spine|pulse|andon|heartbeat|watchdog' && { echo "chorus"; return; }
  echo "$title_lower" | grep -qE 'board|card|gate|hook|wip|kanban|vikunja|demo.*gate|acceptance' && { echo "chorus"; return; }
  echo "$title_lower" | grep -qE 'convergence|icd|ontology|owl|rdf|sparql|domain.*service|domain.*page' && { echo "convergence"; return; }
  echo "$title_lower" | grep -qE 'werk|flow.*metric|value.*stream' && { echo "chorus"; return; }
  echo "$title_lower" | grep -qE 'search|embedding|chorus.*search|knowledge.*graph' && { echo "search"; return; }
  echo "$title_lower" | grep -qE 'property|garden|plant|landscape' && { echo "property"; return; }
  echo "$title_lower" | grep -qE 'sexual|intimacy' && { echo "sexuality"; return; }
  echo "$title_lower" | grep -qE 'story|stories|glimmer' && { echo "stories"; return; }
  echo "$title_lower" | grep -qE 'doc.*catalog|manual|documentation' && { echo "chorus"; return; }
  echo "$title_lower" | grep -qE 'harvest|pipeline|ingest|etl' && { echo "convergence"; return; }
  echo "$title_lower" | grep -qE 'test|jest|coverage|ci.*cd|lint' && { echo "infrastructure"; return; }
  echo "$title_lower" | grep -qE 'mind.*map|knowledge.*graph|graph.*nav|d3|visualization' && { echo "search"; return; }
  echo "$title_lower" | grep -qE 'todo|task|checklist' && { echo "todo"; return; }
  echo "$title_lower" | grep -qE 'solid|webid|pod|linked.*data' && { echo "infrastructure"; return; }
  echo "$title_lower" | grep -qE 'gathering.*phase|vision|product.*phil|roadmap|ux|user.*experience' && { echo "product"; return; }
  echo "$title_lower" | grep -qE 'css|theme|dark.*mode|navbar|layout|responsive|ui.*fix|page.*fix|sitemap|scrape' && { echo "app"; return; }
  echo "$title_lower" | grep -qE 'session|context.*cache|role.*state|team.*scan|cost|billing' && { echo "chorus"; return; }

  echo ""  # Can't infer
}

echo "=== Domain Tag Backfill — $(date '+%Y-%m-%d %H:%M') ==="
echo "Mode: $( $APPLY && echo 'APPLY' || echo 'DRY-RUN' )"
[ -n "$FILTER_DOMAIN" ] && echo "Filter: domain=$FILTER_DOMAIN"
echo ""

# Source env for API access
source /Users/jeffbridwell/CascadeProjects/chorus/.env 2>/dev/null || true
TOKEN="${VIKUNJA_TOKEN_KADE:-${VIKUNJA_TOKEN:-}}"
URL="${VIKUNJA_URL:-http://localhost:3456}"

TOTAL=0
TAGGED=0
INFERRED=0
AMBIGUOUS=0
APPLIED=0

# Fetch all pages of tasks
for page in $(seq 1 30); do
  BATCH=$(curl -sf "${URL}/api/v1/projects/2/tasks?per_page=50&page=${page}" \
    -H "Authorization: Bearer ${TOKEN}" 2>/dev/null)
  [ -z "$BATCH" ] && break

  # Process each untagged Done card
  while IFS=$'\t' read -r card_id card_title; do
    [ -z "$card_id" ] && continue
    TOTAL=$((TOTAL + 1))

    domain=$(infer_domain "$card_title")

    if [ -z "$domain" ]; then
      AMBIGUOUS=$((AMBIGUOUS + 1))
      [ -z "$FILTER_DOMAIN" ] && echo "  ? #${card_id}  ${card_title:0:70}"
      continue
    fi

    # Apply filter if set
    if [ -n "$FILTER_DOMAIN" ] && [ "$domain" != "$FILTER_DOMAIN" ]; then
      INFERRED=$((INFERRED + 1))
      continue
    fi

    INFERRED=$((INFERRED + 1))

    if $APPLY; then
      echo "  TAG #${card_id} → domain:${domain}  (${card_title:0:55})"
      $CARDS update "$card_id" --domain "$domain" >/dev/null 2>&1 && APPLIED=$((APPLIED + 1)) || echo "    FAILED: $card_id"
    else
      echo "  → #${card_id} domain:${domain}  ${card_title:0:55}"
    fi
  done < <(echo "$BATCH" | python3 -c "
import json, sys
tasks = json.load(sys.stdin)
done = [t for t in tasks if t.get('done')]
for t in done:
    labels = t.get('labels') or []
    has_domain = any(l.get('title','').startswith('domain:') for l in labels)
    if not has_domain:
        print(f'{t[\"index\"]}\t{t[\"title\"]}')
" 2>/dev/null)

  BATCH_SIZE=$(echo "$BATCH" | python3 -c "import json,sys;print(len(json.load(sys.stdin)))" 2>/dev/null)
  [ "${BATCH_SIZE:-0}" -lt 50 ] && break
done

echo ""
echo "=== Summary ==="
echo "Untagged Done cards: $TOTAL"
echo "Inferred:            $INFERRED"
echo "Ambiguous:           $AMBIGUOUS"
$APPLY && echo "Applied:             $APPLIED"
echo ""
if ! $APPLY; then
  echo "Run with --apply to tag cards."
  echo "Run with --domain seeds --apply to tag seeds domain first."
fi
