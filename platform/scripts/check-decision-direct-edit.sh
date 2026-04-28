#!/usr/bin/env bash
# check-decision-direct-edit.sh — block direct edits to chorus:Decision/chorus:ADR in TTL (#2485 Move 5).
#
# Per ADR-025 + #2485, Decision and ADR instances live in urn:chorus:instances and are
# written via POST /api/athena/subdomains/loom-decisions/decisions. Hand-editing the
# schema TTL to add/modify these instance triples bypasses validation and re-creates
# the original split-graph problem.
#
# Usage:
#   check-decision-direct-edit.sh <file1> [file2 ...]
#   git diff --cached --name-only | check-decision-direct-edit.sh
#
# Env:
#   REPO_ROOT                       (default: git rev-parse --show-toplevel)
#   DECISION_DIRECT_EDIT_SKIP=1     bypass (e.g. one-time migration commit, schema-only edits)
#
# Exit: 0 if clean, 1 if a watched TTL has new/changed chorus:Decision or chorus:ADR triples.
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
  added=$(git diff --cached -U0 -- "$f" 2>/dev/null | grep -E '^\+' | grep -v '^+++' || true)
  dec_hits=$(echo "$added" | grep -E 'a +chorus:Decision\b' || true)
  adr_hits=$(echo "$added" | grep -E 'a +chorus:ADR\b' || true)
  if [ -n "$dec_hits" ]; then
    echo "decision-direct-edit: $f stages new chorus:Decision instance triples." >&2
    echo "$dec_hits" | sed 's/^/  /' >&2
    violation=1
  fi
  if [ -n "$adr_hits" ]; then
    echo "decision-direct-edit: $f stages new chorus:ADR instance triples." >&2
    echo "$adr_hits" | sed 's/^/  /' >&2
    violation=1
  fi
done <<< "$FILES"

if [ "$violation" -eq 0 ]; then
  exit 0
fi

echo "" >&2
echo "Decisions live in urn:chorus:instances and are written via the Athena API:" >&2
echo "  POST /api/athena/subdomains/loom-decisions/decisions" >&2
echo "" >&2
echo "Override (migration commit / schema-only): export DECISION_DIRECT_EDIT_SKIP=1" >&2

if [ "${DECISION_DIRECT_EDIT_SKIP:-0}" = "1" ]; then
  echo "(DECISION_DIRECT_EDIT_SKIP=1 — bypassed)" >&2
  exit 0
fi

exit 1
