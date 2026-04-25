#!/usr/bin/env bash
# check-principle-direct-edit.sh — block direct edits to chorus:Principle in TTL (#2314, #2470).
#
# Per ADR-025, Principle instances live in urn:chorus:instances and are written via
# POST /api/athena/subdomains/loom-principles/principles. Hand-editing the schema TTL
# to add/modify/delete Principle triples bypasses validation and re-creates the original
# split-graph problem.
#
# Detects three shapes of direct edit:
#   1. Add        — staged '+' line declaring 'a chorus:Principle' (#2314)
#   2. Delete     — staged '-' line declaring 'a chorus:Principle' (#2470)
#   3. Modify     — staged '+' or '-' line whose subject is declared
#                   'a chorus:Principle' elsewhere in the file (#2470)
#
# Usage:
#   check-principle-direct-edit.sh <file1> [file2 ...]
#   git diff --cached --name-only | check-principle-direct-edit.sh
#
# Env:
#   REPO_ROOT                       (default: git rev-parse --show-toplevel)
#   PRINCIPLE_DIRECT_EDIT_SKIP=1    bypass (e.g. one-time migration commit, schema-only edits)
#
# Exit: 0 if clean, 1 if a watched TTL has add/modify/delete of chorus:Principle triples.
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

  # Hand the diff + staged blob to python for subject-aware analysis.
  diff_text=$(git diff --cached -U10000 -- "$f" 2>/dev/null)
  staged_text=$(git show ":${f}" 2>/dev/null)
  [ -z "$diff_text" ] && continue

  out=$(DIFF="$diff_text" STAGED="$staged_text" FILE="$f" python3 <<'PY'
import os, re, sys

diff = os.environ["DIFF"]
staged = os.environ.get("STAGED", "")
fname = os.environ["FILE"]

# Pull subjects declared 'a chorus:Principle' from the staged (post-edit) file.
# Subject is the token at column 0 (or after blank lines) before 'a chorus:Principle'.
principle_subjects = set()
current_subject = None
for line in staged.splitlines():
    stripped = line.strip()
    if not stripped or stripped.startswith("#") or stripped.startswith("@"):
        # Block boundary — clear subject context on blank/directive lines.
        if not stripped:
            current_subject = None
        continue
    # New subject if line starts at column 0 with a non-space.
    if line and not line[0].isspace():
        m = re.match(r"^(\S+)\s", line)
        if m:
            current_subject = m.group(1)
    if current_subject and re.search(r"\ba\s+chorus:Principle\b", line):
        principle_subjects.add(current_subject)
    # End of triple block resets subject.
    if stripped.endswith("."):
        current_subject = None

# Walk the unified diff. Track the new-side subject for each line.
# Unified diff lines: ' ' (context), '+' (added), '-' (removed), '@@' (hunk header).
# We track subject from the post-edit perspective: context + added lines participate;
# removed lines also belong to the prior context's subject (the block they were in).
violations = []  # tuples of (kind, content)

current_subject = None
in_hunk = False
for raw in diff.splitlines():
    if raw.startswith("@@"):
        in_hunk = True
        current_subject = None
        continue
    if not in_hunk:
        continue  # diff header
    if raw.startswith("+++") or raw.startswith("---"):
        continue
    marker = raw[:1]
    body = raw[1:] if marker in (" ", "+", "-") else raw

    stripped = body.strip()
    # Track subject. Blank lines reset. Lines starting at column 0 (ignoring
    # the diff marker) introduce a new subject.
    if not stripped or stripped.startswith("#") or stripped.startswith("@prefix"):
        if not stripped:
            current_subject = None
        # Even @prefix counts as a non-instance line; skip violation checks.
        continue
    if body and not body[0].isspace():
        m = re.match(r"^(\S+)\s", body)
        if m:
            current_subject = m.group(1)

    # Add/delete of the type-declaration line itself.
    if marker in ("+", "-") and re.search(r"\ba\s+chorus:Principle\b", body):
        kind = "add" if marker == "+" else "delete"
        violations.append((kind, body))
    # Modification of any line whose subject is a known Principle (excluding
    # the type-declaration itself, which is handled above).
    elif marker in ("+", "-") and current_subject in principle_subjects:
        violations.append(("modify", body))

    if stripped.endswith("."):
        current_subject = None

if violations:
    print(f"principle-direct-edit: {fname} stages add/modify/delete of chorus:Principle triples.", file=sys.stderr)
    seen = set()
    for kind, body in violations:
        key = (kind, body.strip()[:120])
        if key in seen:
            continue
        seen.add(key)
        print(f"  [{kind}] {body.rstrip()}", file=sys.stderr)
    sys.exit(1)
PY
)
  rc=$?
  [ -n "$out" ] && printf '%s\n' "$out" >&2
  [ "$rc" -ne 0 ] && violation=1
done <<< "$FILES"

if [ "$violation" -eq 0 ]; then
  exit 0
fi

echo "" >&2
echo "Principles live in urn:chorus:instances and are written via the Athena API:" >&2
echo "  POST /api/athena/subdomains/loom-principles/principles" >&2
echo "" >&2
echo "Override (migration commit / schema-only): export PRINCIPLE_DIRECT_EDIT_SKIP=1" >&2
echo "  (legit migrations will trip #2451 boot-hash-drift alert until roles /reboot — expected, not spurious)" >&2

if [ "${PRINCIPLE_DIRECT_EDIT_SKIP:-0}" = "1" ]; then
  echo "(PRINCIPLE_DIRECT_EDIT_SKIP=1 — bypassed)" >&2
  exit 0
fi

exit 1
