#!/usr/bin/env bash
# Hermetic tests for doc-wrong-cabinet-move.sh (#2458).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MOVER="${SCRIPT_DIR}/../scripts/doc-wrong-cabinet-move.sh"

if [ ! -x "$MOVER" ]; then
  echo "FAIL: $MOVER not executable" >&2
  exit 1
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/gathering/data/about"
mkdir -p "$FIXTURE/gathering/public/gathering-docs"
mkdir -p "$FIXTURE/gathering/public/chorus-docs"
mkdir -p "$FIXTURE/chorus/roles/silas/adr"
mkdir -p "$FIXTURE/chorus/designing/docs"
mkdir -p "$FIXTURE/chorus/roles/silas/docs"
mkdir -p "$FIXTURE/chorus/roles/silas/artifacts"
mkdir -p "$FIXTURE/chorus/roles/kade/artifacts"
mkdir -p "$FIXTURE/chorus/roles/wren"

(cd "$FIXTURE/gathering" && git init -q && git config user.email test@test && git config user.name test)
(cd "$FIXTURE/chorus"    && git init -q && git config user.email test@test && git config user.name test)

echo "ADR content"         > "$FIXTURE/gathering/data/about/ADR-999-test.md"
echo "chorus design"       > "$FIXTURE/gathering/public/gathering-docs/chorus-test-design.html"
echo "silas role"          > "$FIXTURE/gathering/public/gathering-docs/silas-role.html"
echo "kade role"           > "$FIXTURE/gathering/public/gathering-docs/kade-role.html"
echo "sequence test"       > "$FIXTURE/gathering/public/gathering-docs/sequence-test.html"
echo "borg test"           > "$FIXTURE/gathering/public/gathering-docs/borg-test.html"
echo "chorus roadmap"      > "$FIXTURE/gathering/public/chorus-docs/chorus-test-roadmap.html"
echo "chorus top"          > "$FIXTURE/gathering/public/chorus-test-top.html"
echo "photo model"         > "$FIXTURE/chorus/roles/silas/docs/photo-test-model.md"
echo "photos diff"         > "$FIXTURE/chorus/roles/silas/docs/photos-test-diff.html"
echo "gathering map"       > "$FIXTURE/chorus/roles/wren/gathering-test-map.md"

(cd "$FIXTURE/gathering" && git add -A && git commit -q -m init)
(cd "$FIXTURE/chorus"    && git add -A && git commit -q -m init)

# Collision case A: source is newer than destination — should rename source with -src suffix
echo "dest version (older)" > "$FIXTURE/chorus/designing/docs/chorus-collide.html"
touch -t 202001010101 "$FIXTURE/chorus/designing/docs/chorus-collide.html"
echo "src version (newer)"  > "$FIXTURE/gathering/public/chorus-docs/chorus-collide.html"
# Collision case B: destination is newer (stale fork in source) — should drop source
echo "src version (older, stale)"   > "$FIXTURE/gathering/public/chorus-docs/chorus-stale.html"
touch -t 202001010101 "$FIXTURE/gathering/public/chorus-docs/chorus-stale.html"
echo "dest version (newer, canonical)" > "$FIXTURE/chorus/designing/docs/chorus-stale.html"
(cd "$FIXTURE/gathering" && git add -A && git commit -q -m add-collide)
(cd "$FIXTURE/chorus"    && git add -A && git commit -q -m add-collide)

TSV="$FIXTURE/inventory.tsv"
cat > "$TSV" <<EOF
gathering	data/about/ADR-999-test.md	wrong-cabinet	chorus		N
gathering	public/gathering-docs/chorus-test-design.html	wrong-cabinet	chorus		N
gathering	public/gathering-docs/silas-role.html	wrong-cabinet	chorus		N
gathering	public/gathering-docs/kade-role.html	wrong-cabinet	chorus		N
gathering	public/gathering-docs/sequence-test.html	wrong-cabinet	chorus		N
gathering	public/gathering-docs/borg-test.html	wrong-cabinet	chorus		N
gathering	public/chorus-docs/chorus-test-roadmap.html	wrong-cabinet	chorus		N
gathering	public/chorus-test-top.html	wrong-cabinet	chorus		N
gathering	public/chorus-docs/chorus-collide.html	wrong-cabinet	chorus		N
gathering	public/chorus-docs/chorus-stale.html	wrong-cabinet	chorus		N
chorus	roles/silas/docs/photo-test-model.md	wrong-cabinet	gathering		N
chorus	roles/silas/docs/photos-test-diff.html	wrong-cabinet	gathering		N
chorus	roles/wren/gathering-test-map.md	wrong-cabinet	gathering		N
EOF

GATHERING_REPO="$FIXTURE/gathering" CHORUS_REPO="$FIXTURE/chorus" INPUT="$TSV" "$MOVER" >/dev/null 2>&1

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}
exists() { [ -e "$1" ] && echo Y || echo N; }

check "ADR moved to chorus/roles/silas/adr" Y "$(exists "$FIXTURE/chorus/roles/silas/adr/ADR-999-test.md")"
check "ADR removed from gathering"          N "$(exists "$FIXTURE/gathering/data/about/ADR-999-test.md")"
check "chorus-*.html → designing/docs"      Y "$(exists "$FIXTURE/chorus/designing/docs/chorus-test-design.html")"
check "silas-role → roles/silas/artifacts"  Y "$(exists "$FIXTURE/chorus/roles/silas/artifacts/silas-role.html")"
check "kade-role → roles/kade/artifacts"    Y "$(exists "$FIXTURE/chorus/roles/kade/artifacts/kade-role.html")"
check "sequence-*.html → designing/docs"    Y "$(exists "$FIXTURE/chorus/designing/docs/sequence-test.html")"
check "borg-*.html → designing/docs"        Y "$(exists "$FIXTURE/chorus/designing/docs/borg-test.html")"
check "chorus-docs/*.html → designing/docs" Y "$(exists "$FIXTURE/chorus/designing/docs/chorus-test-roadmap.html")"
check "public/chorus-*.html → designing"    Y "$(exists "$FIXTURE/chorus/designing/docs/chorus-test-top.html")"
check "photo-* → gathering/gathering-docs"  Y "$(exists "$FIXTURE/gathering/public/gathering-docs/photo-test-model.md")"
check "photos-* → gathering/gathering-docs" Y "$(exists "$FIXTURE/gathering/public/gathering-docs/photos-test-diff.html")"
check "gathering-*.md → gathering/docs"     Y "$(exists "$FIXTURE/gathering/docs/gathering-test-map.md")"

check "collide (src newer): dest preserved"  "dest version (older)" "$(cat "$FIXTURE/chorus/designing/docs/chorus-collide.html" 2>/dev/null)"
COLLIDE_COUNT=$(ls "$FIXTURE/chorus/designing/docs/" 2>/dev/null | grep -c 'chorus-collide.*src')
check "collide (src newer): source → -src"   Y "$([ "$COLLIDE_COUNT" -gt 0 ] && echo Y || echo N)"
check "stale (dest newer): dest preserved"   "dest version (newer, canonical)" "$(cat "$FIXTURE/chorus/designing/docs/chorus-stale.html" 2>/dev/null)"
check "stale (dest newer): src dropped"      N "$(exists "$FIXTURE/gathering/public/chorus-docs/chorus-stale.html")"
STALE_SRC_COUNT=$(ls "$FIXTURE/chorus/designing/docs/" 2>/dev/null | grep -c 'chorus-stale.*src')
check "stale (dest newer): no -src file"     0 "$STALE_SRC_COUNT"

# History preservation: source repo retains the full log (cross-repo moves can't bring git history)
SRC_HIST=$(cd "$FIXTURE/gathering" && git log --all --oneline -- data/about/ADR-999-test.md 2>/dev/null | wc -l | tr -d ' ')
check "source repo retains history"          Y "$([ "$SRC_HIST" -gt 0 ] && echo Y || echo N)"
# Destination: new file is tracked (added, will be committed by acp)
DEST_STAGED=$(cd "$FIXTURE/chorus" && git status --porcelain roles/silas/adr/ADR-999-test.md 2>/dev/null | head -c 2)
check "destination file staged for commit"   "A " "$DEST_STAGED"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
