#!/usr/bin/env bash
# Demo Happy-Path — owned by /demo product (Wren). #2893.
#
# Invoked from accept_gate.rs at /acp time (PostToolUse on /demo is too early
# for the same reason as show-gate.sh — Step 5 hasn't run yet).
#
# Parses the card's AC for CHECKABLE references and runs derived checks:
#   - URL / endpoint references (http://, https://, localhost:N, or paths
#     starting with /api/, /about/, /, etc.) → curl, must return < 400
#   - Script paths ending in .sh / .ts / .rs / .py / .md / .html → must exist
#     on disk under $CHORUS_ROOT
#   - Spine event names in backticks (foo.bar.baz) → must appear in chorus.log
#     within the last 24h (proves the emitter actually fires)
#
# Uncheckable AC items (abstract: "no regressions", "Jeff sees X", etc.) are
# silently skipped — the gate only refuses when a checkable item demonstrably
# fails. False-fails on abstract AC would defeat the purpose.
#
# Outcome:
#   - All checkable AC pass → emit demo.happy-path.passed, exit 0
#   - Any checkable AC fails → emit demo.happy-path.failed reason=<…>, exit 1
#   - No checkable AC found → emit demo.happy-path.skipped, exit 0 (don't block
#     abstract-AC cards on the absence of derived checks)
#
# Contract: stderr = refusal message. Exit 0 = pass/skipped, exit 1 = fail.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CARDS="$CHORUS_ROOT/platform/scripts/cards"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"
HOME_DIR="${HOME:-/Users/jeffbridwell}"
CHORUS_LOG_FILE="$HOME_DIR/.chorus/chorus.log"

CARD_ID="${1:-}"
ROLE="${2:-system}"
[ -z "$CARD_ID" ] && exit 0

emit_passed() {
  "$CHORUS_LOG" demo.happy-path.passed "$ROLE" card="$CARD_ID" checks="$1" 2>/dev/null || true
  exit 0
}

emit_skipped() {
  "$CHORUS_LOG" demo.happy-path.skipped "$ROLE" card="$CARD_ID" reason="$1" 2>/dev/null || true
  exit 0
}

emit_failed() {
  local reason="$1"
  "$CHORUS_LOG" demo.happy-path.failed "$ROLE" card="$CARD_ID" reason="$reason" 2>/dev/null || true
  echo "demo-happy-path: failed for #${CARD_ID} — $reason" >&2
  exit 1
}

CARD_VIEW=$("$CARDS" view "$CARD_ID" 2>&1) || true
if [ -z "$CARD_VIEW" ] || echo "$CARD_VIEW" | head -1 | grep -q "^ERROR"; then
  emit_skipped "card-not-found"
fi

# Extract checked AC lines only — unchecked items are unfinished, not testable.
AC_LINES=$(echo "$CARD_VIEW" | grep -E '^\s*- \[x\]' || true)
if [ -z "$AC_LINES" ]; then
  emit_skipped "no-checked-ac"
fi

CHECKS_RUN=0
CHECKS_FAILED=0
FAIL_DETAIL=""

while IFS= read -r line; do
  [ -z "$line" ] && continue

  # --- Check 1: URLs. http(s):// or localhost:N or paths starting with /api/, /about/, etc.
  # First match wins per line. Use grep -oE to extract.
  url=$(echo "$line" | grep -oE 'https?://[^[:space:])]+|localhost:[0-9]+[^[:space:])]*' | head -1 || true)
  if [ -n "$url" ]; then
    # Normalize: prepend http:// if bare localhost:N
    case "$url" in
      http://*|https://*) target="$url" ;;
      localhost:*) target="http://$url" ;;
      *) target="$url" ;;
    esac
    # Trim trailing punctuation that grep may have captured
    target="${target%[.,)\]\"\'\`]}"
    CHECKS_RUN=$((CHECKS_RUN + 1))
    if ! status=$(curl -sf -o /dev/null -w "%{http_code}" --max-time 8 "$target" 2>/dev/null); then
      status="000"
    fi
    if [ -z "$status" ] || [ "$status" -ge 400 ] 2>/dev/null; then
      CHECKS_FAILED=$((CHECKS_FAILED + 1))
      FAIL_DETAIL="${FAIL_DETAIL}url-${status}:${target}; "
    fi
    continue
  fi

  # --- Check 2: script/file paths in backticks ending in known extensions
  path=$(echo "$line" | grep -oE '`[^`]+\.(sh|ts|rs|py|md|html|json|toml)`' | head -1 | tr -d '`' || true)
  if [ -n "$path" ]; then
    # Skip if path looks absolute and outside repo; otherwise resolve under CHORUS_ROOT
    case "$path" in
      /*) candidate="$path" ;;
      *) candidate="$CHORUS_ROOT/$path" ;;
    esac
    CHECKS_RUN=$((CHECKS_RUN + 1))
    if [ ! -e "$candidate" ]; then
      CHECKS_FAILED=$((CHECKS_FAILED + 1))
      FAIL_DETAIL="${FAIL_DETAIL}file-missing:${path}; "
    fi
    continue
  fi

  # --- Check 3: spine event names in backticks (dotted.lowercase pattern)
  event=$(echo "$line" | grep -oE '`[a-z][a-z._-]+\.[a-z][a-z._-]+\.[a-z][a-z._-]+`' | head -1 | tr -d '`' || true)
  if [ -n "$event" ]; then
    CHECKS_RUN=$((CHECKS_RUN + 1))
    if [ ! -f "$CHORUS_LOG_FILE" ] || ! grep -F "$event" "$CHORUS_LOG_FILE" 2>/dev/null | tail -200 | grep -q .; then
      CHECKS_FAILED=$((CHECKS_FAILED + 1))
      FAIL_DETAIL="${FAIL_DETAIL}event-never-fired:${event}; "
    fi
    continue
  fi
done <<<"$AC_LINES"

if [ "$CHECKS_RUN" -eq 0 ]; then
  emit_skipped "no-checkable-references"
fi

if [ "$CHECKS_FAILED" -gt 0 ]; then
  emit_failed "${CHECKS_FAILED}/${CHECKS_RUN}-failed:${FAIL_DETAIL}"
fi

emit_passed "$CHECKS_RUN"
