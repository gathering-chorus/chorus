#!/usr/bin/env bash
# Hermetic tests for doc-triage.sh (#2459).
# Fixture: 10 unfiled docs exercising each rule branch + 1 override.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TRIAGE="${SCRIPT_DIR}/../scripts/doc-triage.sh"

if [ ! -x "$TRIAGE" ]; then
  echo "FAIL: $TRIAGE not executable" >&2
  exit 1
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/chorus/roles/silas/briefs"
mkdir -p "$FIXTURE/chorus/roles/wren/briefs"
mkdir -p "$FIXTURE/chorus/roles/wren"
mkdir -p "$FIXTURE/chorus/directing/products/roles"
mkdir -p "$FIXTURE/chorus/platform/tests/docs"
mkdir -p "$FIXTURE/chorus/designing/docs"
mkdir -p "$FIXTURE/chorus/knowledge"

(cd "$FIXTURE/chorus" && git init -q && git config user.email test@test && git config user.name test)

echo "brief 1"       > "$FIXTURE/chorus/roles/silas/briefs/2026-04-01-note.md"
echo "brief 2"       > "$FIXTURE/chorus/roles/silas/briefs/2026-04-02-note.md"
echo "brief 3"       > "$FIXTURE/chorus/roles/wren/briefs/2026-04-03-note.md"
echo "book outline"  > "$FIXTURE/chorus/roles/wren/book-outline.md"
echo "service brief" > "$FIXTURE/chorus/roles/wren/cockpit-brief.md"
echo "old"           > "$FIXTURE/chorus/roles/wren/SUPERSEDED-old-plan.md"
echo "draft.old"     > "$FIXTURE/chorus/roles/wren/draft-old-2026-01.md"
echo "test docs"     > "$FIXTURE/chorus/platform/tests/docs/TESTING.md"
echo "directing"     > "$FIXTURE/chorus/directing/products/roles/silas-brief.md"
echo "exception"     > "$FIXTURE/chorus/roles/wren/normally-internal.md"

cat > "$FIXTURE/chorus/knowledge/doc-triage-overrides.tsv" <<EOF
roles/wren/normally-internal.md	move-to:designing/docs	jeff-forced-canonical
EOF

(cd "$FIXTURE/chorus" && git add -A && git commit -q -m init)

cat > "$FIXTURE/chorus/knowledge/doc-inventory.tsv" <<EOF
chorus	roles/silas/briefs/2026-04-01-note.md	unfiled	chorus		N
chorus	roles/silas/briefs/2026-04-02-note.md	unfiled	chorus		N
chorus	roles/wren/briefs/2026-04-03-note.md	unfiled	chorus		N
chorus	roles/wren/book-outline.md	unfiled	ambiguous		N
chorus	roles/wren/cockpit-brief.md	unfiled	ambiguous		N
chorus	roles/wren/SUPERSEDED-old-plan.md	unfiled	ambiguous		N
chorus	roles/wren/draft-old-2026-01.md	unfiled	ambiguous		N
chorus	platform/tests/docs/TESTING.md	unfiled	chorus		N
chorus	directing/products/roles/silas-brief.md	unfiled	ambiguous		N
chorus	roles/wren/normally-internal.md	unfiled	ambiguous		N
EOF

PLAN="$FIXTURE/chorus/knowledge/doc-triage-plan.tsv"
CHORUS_REPO="$FIXTURE/chorus" "$TRIAGE" --dry >/dev/null 2>&1
[ -f "$PLAN" ] || { echo "FAIL: plan TSV not written"; exit 1; }

decision() { awk -F'\t' -v p="$1" '$1==p {print $2}' "$PLAN"; }

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

# Briefs fall through to keep (inventory excludes them upstream — if they slip through, we don't mutate)
check "brief silas keep 1"      "keep"       "$(decision 'roles/silas/briefs/2026-04-01-note.md')"
check "brief silas keep 2"      "keep"       "$(decision 'roles/silas/briefs/2026-04-02-note.md')"
check "brief wren keep"         "keep"       "$(decision 'roles/wren/briefs/2026-04-03-note.md')"
check "book-outline move"       "move-to:designing/docs" "$(decision 'roles/wren/book-outline.md')"
check "cockpit-brief move"      "move-to:designing/docs" "$(decision 'roles/wren/cockpit-brief.md')"
check "SUPERSEDED retire"       "retire"     "$(decision 'roles/wren/SUPERSEDED-old-plan.md')"
check "draft-old retire"        "retire"     "$(decision 'roles/wren/draft-old-2026-01.md')"
check "tests/docs keep"         "keep"       "$(decision 'platform/tests/docs/TESTING.md')"
check "directing *-brief moves (filename rule fires if path slipped inventory exclude)" "move-to:designing/docs" "$(decision 'directing/products/roles/silas-brief.md')"
check "override wins over rule" "move-to:designing/docs" "$(decision 'roles/wren/normally-internal.md')"

# Apply (non-dry): assert side-effects
CHORUS_REPO="$FIXTURE/chorus" "$TRIAGE" >/dev/null 2>&1
check "move-to: book-outline at new path"  Y "$([ -f "$FIXTURE/chorus/designing/docs/book-outline.md" ] && echo Y || echo N)"
check "move-to: book-outline removed from old" N "$([ -f "$FIXTURE/chorus/roles/wren/book-outline.md" ] && echo Y || echo N)"
check "retire: old file removed from tree" N "$([ -f "$FIXTURE/chorus/roles/wren/SUPERSEDED-old-plan.md" ] && echo Y || echo N)"
check "keep: file stays"                   Y "$([ -f "$FIXTURE/chorus/platform/tests/docs/TESTING.md" ] && echo Y || echo N)"
check "brief stays untouched"              Y "$([ -f "$FIXTURE/chorus/roles/silas/briefs/2026-04-01-note.md" ] && echo Y || echo N)"

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
