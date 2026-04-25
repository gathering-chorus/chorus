#!/usr/bin/env bash
# check-principle-direct-edit.sh — block direct edits to chorus:Principle in TTL (#2314).
#
# Per ADR-025, Principle instances live in urn:chorus:instances and are written via
# POST /api/athena/subdomains/loom-principles/principles. Hand-editing the schema TTL
# to add/modify Principle triples bypasses validation and re-creates the original
# split-graph problem.
#
# Usage:
#   check-principle-direct-edit.sh <file1> [file2 ...]
#   git diff --cached --name-only | check-principle-direct-edit.sh
#
# Env:
#   REPO_ROOT                       (default: git rev-parse --show-toplevel)
#   PRINCIPLE_DIRECT_EDIT_SKIP=1    bypass (e.g. one-time migration commit, schema-only edits)
#
# Exit: 0 if clean, 1 if a watched TTL has new/changed chorus:Principle triples.
set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"

WATCHED="roles/silas/ontology/chorus.ttl"

FILES=""
if [ $# -gt 0 ]; then
  FILES=$(printf '%s\n' "$@")
else
  FILES=$(cat)
fi

violation=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  [ "$f" = "$WATCHED" ] || continue
  # Look at staged diff for added lines mentioning chorus:Principle as a type or
  # touching an instance (a chorus:Principle / skos:broader of a Principle).
  added=$(git diff --cached -U0 -- "$f" 2>/dev/null | grep -E '^\+' | grep -v '^+++' || true)
  # Watch only added lines that declare an instance of chorus:Principle.
  hits=$(echo "$added" | grep -E 'a +chorus:Principle\b' || true)
  if [ -n "$hits" ]; then
    echo "principle-direct-edit: $f stages new chorus:Principle instance triples." >&2
    echo "$hits" | sed 's/^/  /' >&2
    violation=1
  fi
done <<< "$FILES"

if [ "$violation" -eq 0 ]; then
  exit 0
fi

echo "" >&2
echo "Principles live in urn:chorus:instances and are written via the Athena API:" >&2
echo "  POST /api/athena/subdomains/loom-principles/principles" >&2
echo "" >&2
echo "Override (migration commit / schema-only): export PRINCIPLE_DIRECT_EDIT_SKIP=1" >&2

if [ "${PRINCIPLE_DIRECT_EDIT_SKIP:-0}" = "1" ]; then
  echo "(PRINCIPLE_DIRECT_EDIT_SKIP=1 — bypassed)" >&2
  exit 0
fi

exit 1
