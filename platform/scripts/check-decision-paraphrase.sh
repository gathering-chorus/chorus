#!/usr/bin/env bash

# #2485 Move 4 — cite-by-ID lint for decisions.
#
# Flags files where a decision label appears WITHOUT its canonical ID anywhere in
# the same file. Decisions (DECs and ADRs) are canonical in roles/wren/decisions.md
# and the graph; callers should reference by ID — either as a bare ID ("DEC-093",
# "ADR-026") or alongside the label ("Domain Endpoints (DEC-093)"). Pure paraphrase
# without ID is what this lint catches.
#
# Allow-list (legitimate verbatim appearances regardless of ID):
#   - roles/wren/decisions.md (canonical DEC source)
#   - roles/*/adr/* (canonical ADR files; their own title is allowed)
#   - knowledge/doc-inventory.tsv (file index)
#   - platform/logs/* (frozen)
#   - platform/api/dist/ (build artifacts)
#   - .git/
#
# Pattern: same shape as platform/scripts/check-principle-direct-edit.sh (#2314).

set -uo pipefail

API_URL="${API_URL:-http://localhost:3340}"
REPO_ROOT="${REPO_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

ALLOW_PATTERNS=(
  '^roles/wren/decisions\.md$'
  '^roles/[^/]+/adr/'
  'briefs-archive/'                              # frozen historical briefs (any role)
  'briefs/archive/'                              # frozen historical briefs (subdir variant)
  '^roles/wren/artifacts/chorus-toc\.html$'      # generated TOC; regen is separate concern
  '^roles/wren/artifacts/wren-claudemd-annotated\.html$'
  '^knowledge/doc-inventory\.tsv$'
  '^platform/logs/'
  '^platform/api/dist/'
  '^designing/decisions/DEC-[0-9]+'
  '^\.git/'
)

is_allowed() {
  local path="$1"
  for pat in "${ALLOW_PATTERNS[@]}"; do
    if [[ "$path" =~ $pat ]]; then
      return 0
    fi
  done
  return 1
}

# Extract canonical ID variants for a decision (e.g. dec-093 → DEC-093, dec-93)
id_patterns() {
  local id="$1"
  # id format from API is "dec-NNN" or "adr-NNN" lowercase; bash 3.2 needs `tr`
  local upper
  upper=$(echo "$id" | tr '[:lower:]' '[:upper:]')
  local stripped
  stripped=$(echo "$upper" | sed 's/-0*\([1-9]\)/-\1/')
  if [ "$upper" = "$stripped" ]; then
    echo "$upper"
  else
    echo "$upper|$stripped"
  fi
}

LABELS=$(curl -s --max-time 5 "$API_URL/api/athena/subdomains/loom-decisions/decisions" | python3 -c "
import json, sys
d = json.load(sys.stdin)
decs = d.get('data',{}).get('decisions',[])
for dec in decs:
    label = dec.get('label','').strip()
    if len(label) >= 20:
        print(f\"{dec.get('id','')}\\t{label}\")
" 2>/dev/null)

if [ -z "$LABELS" ]; then
  echo "ERROR: could not fetch decision labels from $API_URL/api/athena/subdomains/loom-decisions/decisions"
  exit 2
fi

LABEL_COUNT=$(echo "$LABELS" | wc -l | tr -d ' ')
echo "=== check-decision-paraphrase ($LABEL_COUNT labels checked) ==="

violations=0
while IFS=$'\t' read -r id label; do
  [ -z "$label" ] && continue
  IDS=$(id_patterns "$id")
  while IFS= read -r path; do
    [ -z "$path" ] && continue
    rel="${path#"$REPO_ROOT"/}"
    if is_allowed "$rel"; then continue; fi
    # Check if the canonical ID appears anywhere in this file.
    if grep -qE "$IDS" "$path" 2>/dev/null; then
      # Label + ID both present → citation-with-context, allowed.
      continue
    fi
    echo "  PARAPHRASE: '$label' (canonical: $id) found in $rel without citation by ID"
    violations=$((violations+1))
  done < <(grep -rl --include='*.md' --include='*.html' --exclude-dir=.git --exclude-dir=node_modules --fixed-strings "$label" "$REPO_ROOT/roles" "$REPO_ROOT/designing" "$REPO_ROOT/docs" "$REPO_ROOT/knowledge" "$REPO_ROOT/platform/api/public" 2>/dev/null)
done <<< "$LABELS"

echo
if [ "$violations" -eq 0 ]; then
  echo "PASS: no decision-label paraphrase without ID citation"
  exit 0
else
  echo "FAIL: $violations paraphrase violation(s) — add the canonical ID (DEC-NNN or ADR-NNN) alongside the label"
  exit 1
fi
