#!/bin/bash
# #3598 — Retirement gate. Deleting a surface (script / gate / hook / source)
# must delete-or-repoint its referencing tests in the SAME change. Otherwise the
# test outlives its surface and fails every run thereafter — the rot that fed the
# nightly false-reds (git-queue.sh, show-gate.sh, done-gate.sh were deleted but
# their .bats were left behind, red forever). This gate fires at the moment of
# deletion: if any test still references a just-deleted surface, block.
#
# Deletions come from $RETGATE_DELETED (space/newline list, for tests/explicit)
# or, by default, the staged diff. Exit 0 = clean; exit 1 = orphaned test(s).
set -u

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

if [ -n "${RETGATE_DELETED:-}" ]; then
  # shellcheck disable=SC2206
  deleted=( ${RETGATE_DELETED} )
else
  deleted=()
  while IFS= read -r line; do [ -n "$line" ] && deleted+=( "$line" ); done \
    < <(git -C "$CHORUS_ROOT" diff --cached --diff-filter=D --name-only 2>/dev/null)
fi

violations=0
for f in "${deleted[@]:-}"; do
  [ -n "$f" ] || continue
  base=$(basename "$f")
  # only surfaces a test actually exercises — not docs/data/config artifacts
  case "$base" in
    *.sh|*.ts|*.rs|*.py|pre-push|pre-commit) ;;
    *) continue ;;
  esac
  # any test file (.bats / test-*.sh) still naming the deleted surface?
  while IFS= read -r tf; do
    [ -n "$tf" ] || continue
    echo "  RETIREMENT-GATE: $tf still references deleted surface $f" >&2
    violations=$(( violations + 1 ))
  done < <(grep -rlF "$base" "$CHORUS_ROOT" --include='*.bats' --include='test-*.sh' 2>/dev/null)
done

if [ "$violations" -gt 0 ]; then
  echo "🪦 retirement-gate BLOCKED: $violations test(s) still reference a deleted surface." >&2
  echo "   Retire or repoint them in THIS change — a deleted surface must not leave orphaned tests (#3598)." >&2
  exit 1
fi
exit 0
