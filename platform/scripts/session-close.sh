#!/bin/bash
# session-close.sh — unified close-out: checks + board audit + commit
# Replaces 3 separate model tool calls with 1. (#1866)
#
# Usage: session-close.sh <role> "<commit summary>"

set -uo pipefail

ROLE="${1:-}"
SUMMARY="${2:-session reboot}"

if [[ -z "$ROLE" ]] || [[ ! "$ROLE" =~ ^(wren|silas|kade)$ ]]; then
  echo "Usage: session-close.sh <role> \"<summary>\"" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Suppress intermediate output — errors only, one summary line (#1866 AC6)
ERRORS=""

# 1. Session close checks
OUT=$("$SCRIPT_DIR/session-close-thin.sh" "$ROLE" 2>&1) || ERRORS="${ERRORS}close: ${OUT}\n"

# 2. Board audit-close
OUT=$("$SCRIPT_DIR/cards" audit-close "$ROLE" 2>&1) || ERRORS="${ERRORS}audit: ${OUT}\n"

# 3. Commit via git-queue
OUT=$("$SCRIPT_DIR/git-queue.sh" commit "$ROLE" "${ROLE}: ${SUMMARY}" 2>&1) || ERRORS="${ERRORS}commit: ${OUT}\n"

if [ -n "$ERRORS" ]; then
  printf "Close: errors\n%b" "$ERRORS" >&2
  exit 1
fi
echo "Close: done"
