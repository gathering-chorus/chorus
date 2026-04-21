#!/usr/bin/env bash
# sweep-test-fixtures.sh — #2428
# Hard-deletes test-fixture cards whose title begins with [e2e-*] or [demo-*],
# with one pinned exception: the singleton [e2e-sentinel] fixture.
# Uses `cards delete` CLI — auth stays inside the TS client.
# Dry-run by default; pass --apply to actually delete.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CARDS_BIN="$CHORUS_ROOT/platform/scripts/cards"

APPLY=0
if [ "${1:-}" = "--apply" ]; then APPLY=1; fi

# Collect candidates via the CLI's list output — display indices, not API IDs.
# Title filter applied in awk; excludes the sentinel by prefix.
ROWS=$(bash "$CARDS_BIN" list 2>/dev/null | awk '
  /^[[:space:]]*[0-9]+[[:space:]]+\[e2e-|^[[:space:]]*[0-9]+[[:space:]]+\[demo-/ {
    if ($0 !~ /\[e2e-sentinel\]/) { print $1 " " $2 }
  }
')

COUNT=$(printf '%s\n' "$ROWS" | grep -c . || true)
echo "Found: $COUNT test-fixture candidates (sentinel excluded)"
if [ "$COUNT" = "0" ]; then
  echo "Nothing to sweep."
  exit 0
fi

if [ "$APPLY" = "0" ]; then
  echo ""
  echo "DRY-RUN — first 10 previews:"
  printf '%s\n' "$ROWS" | head -10 | awk '{printf "  #%s  %.70s\n", $1, substr($0, index($0,$2))}'
  echo ""
  echo "Re-run with --apply to hard-delete all $COUNT."
  exit 0
fi

echo "APPLY mode — hard-deleting $COUNT cards via cards CLI..."
deleted=0
failed=0
while read -r id _rest; do
  [ -z "$id" ] && continue
  if bash "$CARDS_BIN" delete "$id" >/dev/null 2>&1; then
    deleted=$((deleted + 1))
  else
    failed=$((failed + 1))
    echo "  failed id=$id"
  fi
done <<< "$ROWS"

echo ""
echo "Deleted: $deleted   Failed: $failed"
