#!/usr/bin/env bash
# close-out.sh — /close skill implementation (#2230)
#
# Collapses Hard 5 close-out into one command. Writes 3 artifacts from
# a role-supplied one-paragraph summary + board state:
#   1. roles/<role>/next-session.md          (handoff)
#   2. activity.md (appended)                 (team audit log)
#   3. roles/<role>/journal/YYYY-MM-DD.md     (journal, appended if exists)
#
# Then invokes existing session-close.sh (board audit + commit via git-queue).
#
# Usage:
#   close-out.sh <role> "paragraph" [--dry-run]
#
# Arguments:
#   role        wren | silas | kade
#   paragraph   one-paragraph session summary (required, non-empty)
#   --dry-run   print planned writes, touch nothing, do not commit
#
# Env:
#   CHORUS_ROOT  repo root (default: /Users/jeffbridwell/CascadeProjects/chorus)

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

usage() {
  cat <<EOF >&2
Usage: close-out.sh <role> "paragraph" [--dry-run]

Arguments:
  role        wren | silas | kade
  paragraph   one-paragraph session summary (required, non-empty)
  --dry-run   print planned writes, touch nothing, no commit

Writes:
  \$CHORUS_ROOT/roles/<role>/next-session.md       (overwrites)
  \$CHORUS_ROOT/activity.md                        (appends one line)
  \$CHORUS_ROOT/roles/<role>/journal/YYYY-MM-DD.md (appends or creates)

Invokes session-close.sh (board audit + git-queue commit) unless --dry-run.
EOF
  exit 2
}

ROLE="${1:-}"
PARAGRAPH="${2:-}"
DRY_RUN=0
for arg in "${@:3}"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    *) echo "close-out: unknown flag: $arg" >&2; usage ;;
  esac
done

# Validate role
case "$ROLE" in
  wren|silas|kade) ;;
  *) echo "close-out: role must be wren, silas, or kade (got: '${ROLE}')" >&2; usage ;;
esac

# Validate paragraph
if [ -z "$PARAGRAPH" ]; then
  echo "close-out: paragraph is required (got empty string)" >&2
  usage
fi

NEXT_SESSION="${CHORUS_ROOT}/roles/${ROLE}/next-session.md"
ACTIVITY="${CHORUS_ROOT}/../activity.md"
TODAY=$(date '+%Y-%m-%d')
TIMESTAMP=$(date '+%Y-%m-%d %H:%M')
JOURNAL_DIR="${CHORUS_ROOT}/roles/${ROLE}/journal"
JOURNAL="${JOURNAL_DIR}/${TODAY}.md"

if [ "$DRY_RUN" = "1" ]; then
  echo "close-out: DRY RUN — nothing written"
  echo "  would write: ${NEXT_SESSION}"
  echo "  would append: ${ACTIVITY}"
  echo "  would write/append: ${JOURNAL}"
  echo "  would invoke: session-close.sh ${ROLE}"
  exit 0
fi

# Derive board state — best-effort; tolerate command absence in tests
CARDS_CLI="${CHORUS_ROOT}/platform/scripts/cards"
WIP_LIST=""
NEXT_LIST=""
if [ -x "$CARDS_CLI" ]; then
  WIP_LIST=$("$CARDS_CLI" list --status WIP 2>/dev/null | grep -E "^\s+[0-9]+\s" | grep -i "${ROLE}" | head -10 || true)
  NEXT_LIST=$("$CARDS_CLI" list --status Next 2>/dev/null | grep -E "^\s+[0-9]+\s" | grep -i "${ROLE}" | head -5 || true)
fi

# 1. next-session.md (overwrites — this is the handoff for the next session)
mkdir -p "$(dirname "$NEXT_SESSION")"
{
  echo "# $(echo "$ROLE" | awk '{print toupper(substr($0,1,1)) substr($0,2)}') — Next Session"
  echo ""
  echo "## Session close ${TIMESTAMP}"
  echo ""
  echo "${PARAGRAPH}"
  echo ""
  if [ -n "$WIP_LIST" ]; then
    echo "## WIP (still in progress)"
    echo ""
    echo "$WIP_LIST" | sed 's/^/- /'
    echo ""
  fi
  if [ -n "$NEXT_LIST" ]; then
    echo "## Next (queued)"
    echo ""
    echo "$NEXT_LIST" | sed 's/^/- /'
    echo ""
  fi
} > "$NEXT_SESSION"

# 2. activity.md — append one line (shared audit log)
mkdir -p "$(dirname "$ACTIVITY")"
# Collapse paragraph newlines for one-line entry
SUMMARY=$(echo "$PARAGRAPH" | tr '\n' ' ' | sed 's/  */ /g')
echo "- ${TIMESTAMP} [${ROLE}] close → ${SUMMARY}" >> "$ACTIVITY"

# 3. journal — append if today's entry exists, create otherwise
mkdir -p "$JOURNAL_DIR"
if [ -f "$JOURNAL" ]; then
  {
    echo ""
    echo "---"
    echo ""
    echo "## Session close ${TIMESTAMP}"
    echo ""
    echo "${PARAGRAPH}"
  } >> "$JOURNAL"
else
  {
    echo "# ${TODAY} — $(echo "$ROLE" | awk '{print toupper(substr($0,1,1)) substr($0,2)}') Journal"
    echo ""
    echo "## Session close ${TIMESTAMP}"
    echo ""
    echo "${PARAGRAPH}"
  } > "$JOURNAL"
fi

# 4. Invoke existing session-close (audit + commit) — its errors are its own
SESSION_CLOSE="${CHORUS_ROOT}/platform/scripts/session-close.sh"
if [ -x "$SESSION_CLOSE" ]; then
  # First line of paragraph becomes the commit summary
  COMMIT_SUMMARY=$(echo "$PARAGRAPH" | head -1 | cut -c1-120)
  "$SESSION_CLOSE" "$ROLE" "close — ${COMMIT_SUMMARY}" || {
    echo "close-out: session-close.sh reported errors (artifacts still written above)" >&2
  }
else
  echo "close-out: session-close.sh not executable — artifacts written, commit skipped" >&2
fi

echo "Close: ${ROLE} — artifacts written (next-session, activity, journal)"
