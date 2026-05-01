#!/usr/bin/env bash
# branch-check.sh — single-source branch-prefix invariant for the commits
# substrate (#2639). Sourced from:
#   - platform/scripts/git-queue.sh check_branch()  (#2580 queue-path)
#   - platform/hooks/pre-push                       (#2598 bypass-path)
#
# Usage as a script:
#   branch-check.sh <role> <branch>
#     exits 0 if branch matches <role>/* prefix
#     exits 1 with a message on stderr otherwise
#
# Usage as a sourced function:
#   source branch-check.sh
#   if branch_check_match "$ROLE" "$BRANCH"; then ...; fi
#
# Invariant: a role-owned branch starts with `<role>/`. Any non-matching
# branch is treated as cross-role contamination at the call site (see #2580
# for the originating incident class).

# Function form — returns 0 (match) or 1 (mismatch). Caller handles UX.
branch_check_match() {
  local role="${1:-}"
  local branch="${2:-}"

  case "$role" in
    kade|wren|silas) ;;
    *) return 1 ;;
  esac

  if [ -z "$branch" ]; then
    return 1
  fi

  case "$branch" in
    "${role}/"*) return 0 ;;
    *) return 1 ;;
  esac
}

# Script form — only runs when invoked directly, not when sourced.
# `${BASH_SOURCE[0]}` differs from `$0` when this file is sourced.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  role="${1:-}"
  branch="${2:-}"

  if [ -z "$role" ] || [ -z "$branch" ]; then
    echo "branch-check: usage: $0 <role> <branch>" >&2
    exit 1
  fi

  case "$role" in
    kade|wren|silas) ;;
    *)
      echo "branch-check: unknown role '$role' (expected: kade|wren|silas)" >&2
      exit 1
      ;;
  esac

  if branch_check_match "$role" "$branch"; then
    exit 0
  else
    echo "branch-check: branch '$branch' does not match role prefix '${role}/' (#2580 invariant)" >&2
    exit 1
  fi
fi
