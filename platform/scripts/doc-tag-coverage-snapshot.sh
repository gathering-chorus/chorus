#!/usr/bin/env bash
# doc-tag-coverage-snapshot.sh — append today's tag coverage to history TSV (#2522).
#
# Run daily (via cron or pre-close), idempotent on date — if today's row
# exists, it gets overwritten. Output appended to:
#   chorus/knowledge/doc-tag-coverage-history.tsv
# Schema: date \t total_docs \t product_tagged \t product_pct \t subdomain_tagged \t subdomain_pct \t drift_count
set -uo pipefail

CHORUS="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
HISTORY="${CHORUS}/knowledge/doc-tag-coverage-history.tsv"
API="${CHORUS_API:-http://localhost:3340}/api/doc-catalog/tags"

DATA=$(curl -s --max-time 10 "$API")
if [ -z "$DATA" ]; then
  echo "ERROR: empty response from $API" >&2
  exit 1
fi

ROW=$(echo "$DATA" | python3 -c "
import json, sys, datetime
d = json.load(sys.stdin)
date = datetime.date.today().isoformat()
total = d['total']
pp = d['coverage']['product']
sp = d['coverage']['subdomain']
drift = len(d.get('drift', []))
print(f'{date}\t{total}\t{pp[\"tagged\"]}\t{pp[\"percent\"]}\t{sp[\"tagged\"]}\t{sp[\"percent\"]}\t{drift}')
")

mkdir -p "$(dirname "$HISTORY")"

# Header if file is empty/missing
if [ ! -s "$HISTORY" ]; then
  echo -e "# date\ttotal\tproduct_tagged\tproduct_pct\tsubdomain_tagged\tsubdomain_pct\tdrift" > "$HISTORY"
fi

# Idempotent on date — strip today's existing row before append
TODAY=$(echo "$ROW" | cut -f1)
TMP=$(mktemp)
grep -v "^$TODAY"$'\t' "$HISTORY" > "$TMP" || true
mv "$TMP" "$HISTORY"
echo "$ROW" >> "$HISTORY"

echo "Snapshot appended: $ROW"
