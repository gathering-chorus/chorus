#!/usr/bin/env bash
# check-catalog-oversize.sh — reject files >2MB in doc-catalog SOURCE_DIRs (#2461).
#
# Root cause: 2026-04-24 push bloat — 74 book PNGs (9-18 MB each) committed to
# platform/api/public/book/images-v2/. Pack grew 340MB. GitHub HTTPS failed RPC 400.
# This gate blocks the regression at commit time.
#
# Usage:
#   check-catalog-oversize.sh <file1> [file2 ...]     # explicit list
#   git diff --cached --name-only | check-catalog-oversize.sh  # stdin
#
# Env:
#   REPO_ROOT          (default: git rev-parse --show-toplevel)
#   MAX_SIZE_BYTES     (default: 2097152 = 2 MB)
#   CATALOG_OVERSIZE_SKIP=1  bypass (intentional large asset)
#
# Exit: 0 if clean, 1 if violations found (unless skip is set).
set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
MAX_SIZE_BYTES="${MAX_SIZE_BYTES:-2097152}"

# Read files from args or stdin
FILES=""
if [ $# -gt 0 ]; then
  FILES=$(printf '%s\n' "$@")
else
  FILES=$(cat)
fi

is_catalog_path() {
  case "$1" in
    designing/docs/*|designing/decisions/*|\
    docs/diagrams/*|\
    roles/silas/docs/*|roles/silas/artifacts/*|roles/silas/adr/*|\
    roles/wren/docs/*|roles/wren/artifacts/*|roles/wren/decisions/*|\
    roles/kade/docs/*|roles/kade/artifacts/*|\
    platform/api/public/*) return 0 ;;
    *) return 1 ;;
  esac
}

VIOLATIONS=""
VIOLATION_COUNT=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  is_catalog_path "$f" || continue
  [ -f "$REPO_ROOT/$f" ] || continue
  size=$(stat -f '%z' "$REPO_ROOT/$f" 2>/dev/null || stat -c '%s' "$REPO_ROOT/$f" 2>/dev/null || echo 0)
  if [ "$size" -gt "$MAX_SIZE_BYTES" ]; then
    mb=$(awk "BEGIN {printf \"%.1f\", $size / 1048576}")
    VIOLATIONS="${VIOLATIONS}  - $f (${mb} MB)
"
    VIOLATION_COUNT=$((VIOLATION_COUNT + 1))
  fi
done <<< "$FILES"

if [ "$VIOLATION_COUNT" -eq 0 ]; then
  exit 0
fi

# Report
echo "catalog-oversize: $VIOLATION_COUNT file(s) exceed ${MAX_SIZE_BYTES} bytes in catalog dirs:" >&2
printf '%s' "$VIOLATIONS" >&2
echo "" >&2
echo "Catalog dirs are for text content. Large binaries belong outside tracked tree (see #2458 push-bloat incident)." >&2
echo "Override: export CATALOG_OVERSIZE_SKIP=1" >&2

if [ "${CATALOG_OVERSIZE_SKIP:-0}" = "1" ]; then
  echo "(CATALOG_OVERSIZE_SKIP=1 — bypassed)" >&2
  exit 0
fi

exit 1
