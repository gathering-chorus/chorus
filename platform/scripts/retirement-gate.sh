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
  # #4345 — a GENERIC file name says nothing on its own: deleting one crate's
  # src/main.rs flagged all 13 tests that mention any main.rs (chorus-inject's,
  # chorus-hooks'…) and blocked a crate retirement. For those names the needle is
  # the last three path parts (chorus-awake/src/main.rs); every other surface is
  # still matched by its base name, which is how tests usually name a script.
  needle="$base"
  case "$base" in
    main.rs|lib.rs|mod.rs|build.rs|index.ts|index.js|server.ts|main.ts|mod.ts|__init__.py|main.py)
      needle=$(printf '%s' "$f" | awk -F/ '{ n=NF; s=$n; if (n>1) s=$(n-1)"/"s; if (n>2) s=$(n-2)"/"s; print s }') ;;
  esac
  # any test file (.bats / test-*.sh) still naming the deleted surface?
  # #3702 — EXCEPT declared absence-guards: a retirement card's own test must name
  # the deleted surface to assert it stays gone (the domain-detail-retired.bats
  # convention, which never collided here only because it guards non-code files).
  # The marker is an explicit opt-in, visible in the test header.
  while IFS= read -r tf; do
    [ -n "$tf" ] || continue
    if head -20 "$tf" | grep -q 'retirement-gate: absence-guard'; then
      continue
    fi
    echo "  RETIREMENT-GATE: $tf still references deleted surface $f" >&2
    violations=$(( violations + 1 ))
  done < <(grep -rlF "$needle" "$CHORUS_ROOT" --include='*.bats' --include='test-*.sh' 2>/dev/null)
done

if [ "$violations" -gt 0 ]; then
  echo "🪦 retirement-gate BLOCKED: $violations test(s) still reference a deleted surface." >&2
  echo "   Retire or repoint them in THIS change — a deleted surface must not leave orphaned tests (#3598)." >&2
  exit 1
fi
exit 0
