#!/usr/bin/env bash
# Hermetic tests for doc-inventory.sh (AC7 of #2457).
# Fixture: 6 files exercising each state.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INVENTORY="${SCRIPT_DIR}/../scripts/doc-inventory.sh"

if [ ! -x "$INVENTORY" ]; then
  echo "FAIL: $INVENTORY not executable" >&2
  exit 1
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# Build fixture: two fake repos with known classifications
mkdir -p "$FIXTURE/gathering/public/gathering-docs"
mkdir -p "$FIXTURE/gathering/public/chorus-docs"
mkdir -p "$FIXTURE/chorus/designing/docs"
mkdir -p "$FIXTURE/chorus/roles/wren"
(cd "$FIXTURE/gathering" && git init -q && git commit -q --allow-empty -m init)
(cd "$FIXTURE/chorus"    && git init -q && git commit -q --allow-empty -m init)

# 1. ok — gathering repo, correct cabinet, has owner
cat > "$FIXTURE/gathering/public/gathering-docs/home-page.md" <<EOF
---
owner: jeff
topic: gathering
status: canonical
---
# Home
EOF

# 2. ok — chorus repo, correct cabinet, has owner
cat > "$FIXTURE/chorus/designing/docs/pulse-design.md" <<EOF
---
owner: silas
topic: pulse
status: canonical
---
# Pulse Service Design
EOF

# 3. wrong-cabinet — chorus-named file in gathering repo (no owner)
cat > "$FIXTURE/gathering/public/gathering-docs/chorus-spine.html" <<EOF
<!DOCTYPE html><html><title>Chorus Spine</title></html>
EOF

# 4. misfiled — in correct repo but no owner front-matter
cat > "$FIXTURE/chorus/designing/docs/orphan-notes.md" <<EOF
# Random chorus notes without front-matter
EOF

# 5. unfiled — chorus repo, role top-level (not in catalog scan dirs)
cat > "$FIXTURE/chorus/roles/wren/book-outline.md" <<EOF
---
owner: wren
topic: book
status: draft
---
# Book outline
EOF

# 6. edge — non-.md/.html file (must be ignored)
echo "binary-ish" > "$FIXTURE/chorus/designing/docs/not-a-doc.bin"

# Run inventory against the fixture
OUT="$FIXTURE/inventory.tsv"
GATHERING_REPO="$FIXTURE/gathering" CHORUS_REPO="$FIXTURE/chorus" OUTPUT="$OUT" \
  "$INVENTORY" >/dev/null 2>&1

pass=0; fail=0
check() {
  local desc="$1"; local expected="$2"; local actual="$3"
  if [ "$expected" = "$actual" ]; then
    pass=$((pass+1))
    echo "  PASS: $desc"
  else
    fail=$((fail+1))
    echo "  FAIL: $desc (expected: $expected, got: $actual)"
  fi
}

# Total rows = 5 (the .bin must be ignored)
rows=$(grep -vc '^#' "$OUT")
check "row count = 5 (ignores .bin)" 5 "$rows"

# State counts
check "ok count"            2 "$(awk -F'\t' '$3=="ok"'            "$OUT" | wc -l | tr -d ' ')"
check "wrong-cabinet count" 1 "$(awk -F'\t' '$3=="wrong-cabinet"' "$OUT" | wc -l | tr -d ' ')"
check "misfiled count"      1 "$(awk -F'\t' '$3=="misfiled"'      "$OUT" | wc -l | tr -d ' ')"
check "unfiled count"       1 "$(awk -F'\t' '$3=="unfiled"'       "$OUT" | wc -l | tr -d ' ')"

# Specific rows
check "chorus-spine classified wrong-cabinet" "wrong-cabinet" \
  "$(awk -F'\t' '$2 ~ /chorus-spine\.html/ {print $3}' "$OUT")"
check "book-outline classified unfiled" "unfiled" \
  "$(awk -F'\t' '$2 ~ /book-outline\.md/ {print $3}' "$OUT")"
check "orphan-notes classified misfiled" "misfiled" \
  "$(awk -F'\t' '$2 ~ /orphan-notes\.md/ {print $3}' "$OUT")"
check "pulse-design owner = silas" "silas" \
  "$(awk -F'\t' '$2 ~ /pulse-design\.md/ {print $5}' "$OUT")"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
