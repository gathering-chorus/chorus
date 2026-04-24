#!/usr/bin/env bash
# doc-inventory.sh — walks gathering + chorus repos, classifies every .md/.html
# into ok | wrong-cabinet | misfiled | unfiled, emits TSV. (#2457)
#
# Usage:
#   doc-inventory.sh                               # write to default OUTPUT
#   OUTPUT=/tmp/foo.tsv doc-inventory.sh           # override
#   GATHERING_REPO=... CHORUS_REPO=... OUTPUT=... doc-inventory.sh  # for tests
#
# TSV columns (tab-separated, no header line by default):
#   repo  path  state  correct-cabinet  owner  in-catalog  topic-tags
set -uo pipefail

GATHERING_REPO="${GATHERING_REPO:-/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site}"
CHORUS_REPO="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
OUTPUT="${OUTPUT:-${CHORUS_REPO}/knowledge/doc-inventory.tsv}"

# Catalog SOURCE_DIRS (from jeff-bridwell-personal-site/src/handlers/doc-catalog.handler.ts)
GATHERING_CATALOG_DIRS=(
  "public"
  "public/gathering-docs"
  "public/chorus-docs"
  "public/akasha"
  "docs"
  "data/about"
)
CHORUS_CATALOG_DIRS=(
  "roles/wren/artifacts"
  "roles/wren/docs"
  "roles/wren/decisions"
  "roles/silas/docs"
  "roles/silas/artifacts"
  "roles/silas/adr"
  "designing/docs"
  "designing/decisions"
  "docs/diagrams"
)

# Chorus-native filename patterns: these should live in chorus repo
CHORUS_PATTERNS='^(chorus-|borg-|silas-|wren-|kade-|adr-|sequence-|icd-)'
# Gathering-native filename patterns: these should live in gathering repo
GATHERING_PATTERNS='^(gathering-|site-|garden-|photo-|photos-|home-|blog-)'

extract_frontmatter() {
  # $1 = file; echoes "owner|topic|status" (pipe-delimited) or empty if no front-matter
  local f="$1"
  # Fast path: skip awk entirely if file doesn't start with ---
  head -c 3 "$f" 2>/dev/null | grep -q '^---' || { echo "|||"; return; }
  head -30 "$f" 2>/dev/null | awk '
    NR==1 { if ($0 != "---") exit; next }
    $0 == "---" { exit }
    /^owner:/  { sub("^owner:[[:space:]]*", "");  gsub(/[[:space:]]+$/, ""); o=$0 }
    /^topic:/  { sub("^topic:[[:space:]]*", "");  gsub(/[[:space:]]+$/, ""); t=$0 }
    /^status:/ { sub("^status:[[:space:]]*", ""); gsub(/[[:space:]]+$/, ""); s=$0 }
    END { print o"|"t"|"s }
  '
}

correct_cabinet_for() {
  # $1 = basename (lowercase)
  local bn="$1"
  if [[ "$bn" =~ $CHORUS_PATTERNS ]]; then
    echo "chorus"
  elif [[ "$bn" =~ $GATHERING_PATTERNS ]]; then
    echo "gathering"
  else
    echo "ambiguous"
  fi
}

in_catalog_dir() {
  # $1 = repo label (gathering|chorus), $2 = relative path from repo root
  # Catalog scanner (doc-catalog.handler.ts) uses readdirSync — top-level only, not recursive.
  # So a file counts as in-catalog iff its immediate parent dir equals a SOURCE_DIR.
  local repo="$1" rel="$2" dir parent
  parent=$(dirname "$rel")
  if [ "$repo" = "gathering" ]; then
    for dir in "${GATHERING_CATALOG_DIRS[@]}"; do
      [ "$parent" = "$dir" ] && { echo "Y"; return; }
    done
  else
    for dir in "${CHORUS_CATALOG_DIRS[@]}"; do
      [ "$parent" = "$dir" ] && { echo "Y"; return; }
    done
  fi
  echo "N"
}

classify_one() {
  # $1 = repo label, $2 = absolute path, $3 = rel path
  local repo="$1" abspath="$2" rel="$3"
  local bn; bn=$(basename "$rel" | tr '[:upper:]' '[:lower:]')
  local cabinet; cabinet=$(correct_cabinet_for "$bn")
  local fm; fm=$(extract_frontmatter "$abspath")
  local owner="${fm%%|*}"
  local rest="${fm#*|}"
  local topic="${rest%%|*}"
  local catalog; catalog=$(in_catalog_dir "$repo" "$rel")
  local hash; hash=$(shasum -a 256 "$abspath" 2>/dev/null | awk '{print substr($1,1,12)}')

  local state
  if [ "$cabinet" != "ambiguous" ] && [ "$cabinet" != "$repo" ]; then
    state="wrong-cabinet"
  elif [ "$catalog" = "N" ]; then
    state="unfiled"
  else
    # In a catalog dir = ok. Missing owner front-matter is not drift.
    state="ok"
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$repo" "$rel" "$state" "$cabinet" "$owner" "$catalog" "$topic" "$hash"
}

walk_repo() {
  # $1 = repo label, $2 = repo root
  local repo="$1" root="$2"
  [ -d "$root" ] || return 0
  find "$root" -type f \( -name '*.md' -o -name '*.html' \) \
    -not -path '*/.git/*' \
    -not -path '*/node_modules/*' \
    -not -path '*/target/*' \
    -not -path '*/dist/*' \
    -not -path '*/build/*' \
    -not -path '*/coverage/*' \
    -not -path '*/.claude/*' \
    -not -path '*/logs/*' \
    -not -path '*/backups/*' \
    -not -path '*/transcripts/*' \
    -not -path '*/journal/*' \
    -not -path '*/fixtures/*' \
    -not -path '*/briefs/*' \
    -not -path '*/briefs-archive/*' \
    -not -path '*/directing/products/roles/*' \
    -not -path '*/messages/*' \
    -not -path '*/.chorus/*' \
    -not -path '*/skills/*' \
    -not -path '*/claudemd/*' \
    -not -path '*/domain-context/*' \
    -not -path '*/reports/*' \
    -not -path '*/plato-report/*' \
    -not -path '*/e2e/screenshots/*' \
    -not -path '*/terraform/*' \
    -not -path '*/tests/docs/*' \
    -not -path '*/tests/fixtures/*' \
    -not -name 'CLAUDE.md' \
    -not -name 'backlog.md' \
    -not -name 'projects.md' \
    -not -name 'stories.md' \
    -not -name 'tech-debt.md' \
    -not -name 'decisions.md' \
    -not -name 'service-manifest.md' \
    -not -name 'scope-ownership.md' \
    -not -name 'role-config-manifest.md' \
    -not -name 'RUNBOOK.md' \
    -not -name 'RUNBOOK.html' \
    -not -name 'TEAM_PROTOCOL.md' \
    -not -name 'team-architecture.md' \
    -not -name 'README.md' \
    -not -name 'TEST.md' \
    -not -name 'test-triage.md' \
    -not -name 'reference-templates.md' \
    -not -name 'working-agreement-*.md' \
    -not -name 'turtle-filesystem-and-ontology.md' \
    -not -name 'next-session.md' \
    -not -name 'next-session.md.consumed' \
    -not -path '*/platform/api/public/*' \
    -not -path '*/ghost_content/*' \
    -not -path '*/jscpd-report/*' \
    -not -path '*/playwright-report/*' \
    -not -path '*/data/pods/*' \
    -not -path '*/data/harvest/*' \
    -not -path '*/directing/clearing/*' \
    -not -path '*/test-output/*' \
    -not -path '*/test-fixtures/*' \
    2>/dev/null | while IFS= read -r f; do
      local rel="${f#"$root"/}"
      classify_one "$repo" "$f" "$rel"
    done
}

mkdir -p "$(dirname "$OUTPUT")"
{
  walk_repo gathering "$GATHERING_REPO"
  walk_repo chorus    "$CHORUS_REPO"
} > "$OUTPUT"

# Summary to stderr
awk -F'\t' '{c[$3]++} END{for (s in c) printf "  %-14s %d\n", s, c[s]}' "$OUTPUT" >&2
echo "→ $OUTPUT ($(wc -l < "$OUTPUT" | tr -d ' ') rows)" >&2
