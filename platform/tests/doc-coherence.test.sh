#!/usr/bin/env bash
# Hermetic tests for doc-coherence.sh (#2461).
# Fixture: inventory TSV with 1 content-dup, 1 basename-dup, 1 clean — coherence reports both.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COHERENCE="${SCRIPT_DIR}/../scripts/doc-coherence.sh"

if [ ! -x "$COHERENCE" ]; then
  echo "FAIL: $COHERENCE not executable" >&2
  exit 1
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/knowledge"

# Inventory TSV: 8 cols = repo path state cabinet owner catalog topic hash
# Two rows with the same hash = content-duplicate
# Two rows with same basename but different hash = basename-duplicate
cat > "$FIXTURE/knowledge/doc-inventory.tsv" <<EOF
chorus	designing/docs/a.md	ok	chorus		Y		aaaa11112222
chorus	roles/silas/docs/a.md	ok	chorus		Y		aaaa11112222
chorus	designing/docs/collide.md	ok	chorus		Y		bbbb22223333
chorus	roles/wren/docs/collide.md	ok	chorus		Y		cccc33334444
chorus	designing/docs/unique.md	ok	chorus		Y		dddd44445555
EOF

REPORT="$FIXTURE/knowledge/doc-coherence.md"

CHORUS_REPO="$FIXTURE" INVENTORY="$FIXTURE/knowledge/doc-inventory.tsv" REPORT="$REPORT" \
  SKIP_HREF_PROBE=1 "$COHERENCE" >/dev/null 2>&1

[ -f "$REPORT" ] || { echo "FAIL: report not written"; exit 1; }

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

# Report structure
check "content-dup section present" 1 "$(grep -c '## Content-hash duplicates' "$REPORT")"
check "basename-dup section present" 1 "$(grep -c '## Basename duplicates' "$REPORT")"
check "content-dup count line" "1" "$(grep 'content-dup-groups:' "$REPORT" | head -1 | awk '{print $2}')"
check "basename-dup count line" "1" "$(grep 'basename-dup-groups:' "$REPORT" | head -1 | awk '{print $2}')"

# Specific entries
check "content-dup lists a.md pair"   1 "$(grep -c 'designing/docs/a.md' "$REPORT")"
check "content-dup lists silas a.md"  1 "$(grep -c 'roles/silas/docs/a.md' "$REPORT")"
check "basename-dup lists collide (header + 2 path lines)" 3 "$(grep -c 'collide.md' "$REPORT")"
check "unique.md absent"              0 "$(grep -c 'unique.md' "$REPORT")"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
