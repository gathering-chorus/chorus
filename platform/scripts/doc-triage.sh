#!/usr/bin/env bash
# doc-triage.sh — KM op 2 (#2459). Triage `unfiled` rows from the inventory TSV
# into one of: internal | move-to:<dir> | retire | keep.
#
# Rules (first-match-wins), after override check:
#   override from chorus/knowledge/doc-triage-overrides.tsv (path\tdecision\treason)
#   retire: filename matches SUPERSEDED-*, ARCHIVE-*, draft-old-*
#   internal: path matches */briefs/*, directing/products/**, */journal/*, */backups/*
#   move-to:designing/docs: roles/*/(book-*|cockpit-*|*-brief|*-design|chorus-*)
#   keep: anything else
#
# Usage:
#   doc-triage.sh                      # apply decisions (real run)
#   doc-triage.sh --dry                # write plan TSV only, no changes
#   CHORUS_REPO=... INVENTORY=... OVERRIDE=... doc-triage.sh   # for tests
#
# Plan TSV columns (tab-separated):
#   path  decision  reason
set -uo pipefail

CHORUS_REPO="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
INVENTORY="${INVENTORY:-${CHORUS_REPO}/knowledge/doc-inventory.tsv}"
OVERRIDE="${OVERRIDE:-${CHORUS_REPO}/knowledge/doc-triage-overrides.tsv}"
PLAN="${PLAN:-${CHORUS_REPO}/knowledge/doc-triage-plan.tsv}"
DRY=0
[ "${1:-}" = "--dry" ] && DRY=1

classify() {
  # $1 = relative path (from chorus repo root)
  local p="$1" bn
  bn=$(basename "$p")

  # Override takes precedence
  if [ -f "$OVERRIDE" ]; then
    local ov
    ov=$(awk -F'\t' -v target="$p" '$1==target {print $2"\t"$3; exit}' "$OVERRIDE")
    if [ -n "$ov" ]; then
      printf '%s\t%s\n' "${ov%%$'\t'*}" "override:${ov##*$'\t'}"
      return
    fi
  fi

  # Retire — filename heuristics
  case "$bn" in
    SUPERSEDED-*|ARCHIVE-*|draft-old-*|*-DEPRECATED.*|*-RETIRED.*)
      echo -e "retire\trule:retire-filename"; return ;;
  esac

  # Internal paths (briefs, journal, claudemd, etc.) are excluded upstream by doc-inventory.sh.
  # Any that slip through get `keep` — we don't mutate files to mark them internal (Jeff, 2026-04-24).

  # Move-to-catalog — role-authored canonical docs
  case "$bn" in
    book-*.md|book-*.html|\
    cockpit-*.md|cockpit-*.html|\
    *-brief.md|*-brief.html|\
    *-design.md|*-design.html|\
    *-proposal.md|*-proposal.html|\
    chorus-*.md|chorus-*.html|\
    canonical-*.md|canonical-*.html)
      echo -e "move-to:designing/docs\trule:authored-canonical"; return ;;
  esac

  # Role-specific content in role dirs (not briefs/journal) → still role-internal
  case "$p" in
    roles/*/artifacts/*|roles/*/docs/*|roles/*/decisions/*)
      echo -e "keep\trule:role-artifact-already-scanned"; return ;;
  esac

  # Fallback
  echo -e "keep\trule:fallback"
}

# Build plan
: > "$PLAN"
while IFS=$'\t' read -r repo path state cabinet owner catalog topic; do
  [ "$state" = "unfiled" ] || continue
  decision=$(classify "$path")
  printf '%s\t%s\n' "$path" "$decision" >> "$PLAN"
done < "$INVENTORY"

if [ "$DRY" = "1" ]; then
  awk -F'\t' '{c[$2]++} END{for(k in c) printf "  %-30s %d\n", k, c[k]}' "$PLAN" >&2
  echo "→ $PLAN ($(wc -l < "$PLAN" | tr -d ' ') rows, dry-run)" >&2
  exit 0
fi

# Apply decisions
cd "$CHORUS_REPO"
applied=0; skipped=0
while IFS=$'\t' read -r path decision reason; do
  [ -z "$path" ] && continue
  case "$decision" in
    move-to:*)
      dest_dir="${decision#move-to:}"
      dest="${dest_dir}/$(basename "$path")"
      mkdir -p "$dest_dir"
      if [ -f "$path" ] && [ ! -e "$dest" ]; then
        git mv "$path" "$dest" 2>/dev/null || mv "$path" "$dest"
        applied=$((applied+1))
      else
        skipped=$((skipped+1))
      fi ;;
    retire)
      if [ -f "$path" ]; then
        git rm -q "$path" 2>/dev/null || rm "$path"
        applied=$((applied+1))
      else
        skipped=$((skipped+1))
      fi ;;
    keep)
      : ;; # no-op
    *)
      skipped=$((skipped+1)) ;;
  esac
done < "$PLAN"

echo "→ $PLAN — $applied applied, $skipped skipped" >&2
awk -F'\t' '{c[$2]++} END{for(k in c) printf "  %-30s %d\n", k, c[k]}' "$PLAN" >&2
