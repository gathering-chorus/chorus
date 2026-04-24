#!/usr/bin/env bash
# Hermetic tests for check-catalog-oversize.sh (#2461).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECK="${SCRIPT_DIR}/../scripts/check-catalog-oversize.sh"

[ -x "$CHECK" ] || { echo "FAIL: $CHECK not executable" >&2; exit 1; }

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/designing/docs"
mkdir -p "$FIXTURE/platform/api/public/book"
mkdir -p "$FIXTURE/roles/silas/adr"
mkdir -p "$FIXTURE/scripts"   # not a catalog dir

# Fixture files:
#  small-doc.md         — 1KB in catalog dir           → pass
#  huge-photo.png       — 3MB in catalog dir           → fail
#  tiny-readme.md       — 500B in catalog dir          → pass
#  big-script.sh        — 5MB in non-catalog dir       → pass (not in catalog)
#  huge-adr.html        — 3MB in catalog (roles/silas/adr) → fail
echo "x" > "$FIXTURE/designing/docs/small-doc.md"
dd if=/dev/zero of="$FIXTURE/designing/docs/huge-photo.png" bs=1024 count=3072 2>/dev/null
echo "y" > "$FIXTURE/platform/api/public/tiny-readme.md"
dd if=/dev/zero of="$FIXTURE/scripts/big-script.sh" bs=1024 count=5120 2>/dev/null
dd if=/dev/zero of="$FIXTURE/roles/silas/adr/huge-adr.html" bs=1024 count=3072 2>/dev/null

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

# Run 1: only clean files — exit 0, no output
cd "$FIXTURE"
out=$(echo -e "designing/docs/small-doc.md\nplatform/api/public/tiny-readme.md" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "all-clean exit 0" 0 "$rc"
check "all-clean no output" "" "$out"

# Run 2: one oversize — exit 1, output mentions the file
out=$(echo -e "designing/docs/small-doc.md\ndesigning/docs/huge-photo.png" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "one-oversize exit 1" 1 "$rc"
check "one-oversize mentions huge-photo" 1 "$(echo "$out" | grep -c 'huge-photo.png')"
check "one-oversize does not mention small-doc" 0 "$(echo "$out" | grep -c 'small-doc.md')"

# Run 3: oversize in non-catalog dir — exit 0 (pass; we only care about catalog dirs)
out=$(echo "scripts/big-script.sh" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "non-catalog oversize exit 0 (not our concern)" 0 "$rc"
check "non-catalog oversize no output" "" "$out"

# Run 4: multiple oversize in multiple catalog dirs — exit 1, reports all
out=$(echo -e "designing/docs/huge-photo.png\nroles/silas/adr/huge-adr.html" | REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "multiple-oversize exit 1" 1 "$rc"
check "multiple-oversize lists huge-photo" 1 "$(echo "$out" | grep -c 'huge-photo.png')"
check "multiple-oversize lists huge-adr"   1 "$(echo "$out" | grep -c 'huge-adr.html')"
check "multiple-oversize count=2"          1 "$(echo "$out" | grep -c '^catalog-oversize: 2 file')"

# Run 5: skip flag bypasses
out=$(echo "designing/docs/huge-photo.png" | CATALOG_OVERSIZE_SKIP=1 REPO_ROOT="$FIXTURE" "$CHECK" 2>&1)
rc=$?
check "skip-flag exit 0" 0 "$rc"
check "skip-flag mentions bypass" 1 "$(echo "$out" | grep -c 'bypassed')"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
