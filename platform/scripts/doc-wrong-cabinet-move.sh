#!/usr/bin/env bash
# doc-wrong-cabinet-move.sh — apply KM wrong-cabinet moves (#2458).
# Reads inventory TSV rows where state=wrong-cabinet, maps each to a destination
# via rules, and does `git mv` in the source repo, then copies into the dest repo.
#
# Moves are cross-repo, so we can't use a single `git mv`. The flow per file:
#   1. In source repo: `git rm` the file (history retained via log --all)
#   2. In destination repo: `git add` the file content at the new path
#   3. Commit in each repo separately (handled by caller/acp, not here)
#
# Rules table at bottom — edit the case statements to extend.
#
# Usage:
#   doc-wrong-cabinet-move.sh                               # real run, default paths
#   INPUT=... GATHERING_REPO=... CHORUS_REPO=... doc-wrong-cabinet-move.sh
#   doc-wrong-cabinet-move.sh --dry                         # print plan only
set -uo pipefail

GATHERING_REPO="${GATHERING_REPO:-/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site}"
CHORUS_REPO="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
INPUT="${INPUT:-${CHORUS_REPO}/knowledge/doc-inventory.tsv}"
DRY=0
[ "${1:-}" = "--dry" ] && DRY=1

# Map source row → destination rel path in the OTHER repo
map_destination() {
  # $1 = source repo (gathering|chorus), $2 = source rel path
  local repo="$1" rel="$2" bn
  bn=$(basename "$rel")

  if [ "$repo" = "gathering" ]; then
    # gathering → chorus
    case "$rel" in
      data/about/ADR-*|data/about/adr-*)    echo "roles/silas/adr/$bn"; return ;;
    esac
    case "$bn" in
      silas-*.html|silas-*.md)              echo "roles/silas/artifacts/$bn"; return ;;
      kade-*.html|kade-*.md)                echo "roles/kade/artifacts/$bn"; return ;;
      wren-*.html|wren-*.md)                echo "roles/wren/artifacts/$bn"; return ;;
      chorus-*|borg-*|sequence-*|icd-*)     echo "designing/docs/$bn"; return ;;
    esac
    # Fallback for other chorus-named gathering-side files
    echo "designing/docs/$bn"
  else
    # chorus → gathering
    case "$bn" in
      photo-*|photos-*)                     echo "public/gathering-docs/$bn"; return ;;
      gathering-*.md|gathering-*.html)      echo "docs/$bn"; return ;;
      home-*|site-*|garden-*|blog-*)        echo "public/gathering-docs/$bn"; return ;;
    esac
    echo "public/gathering-docs/$bn"
  fi
}

sha() { shasum -a 256 "$1" 2>/dev/null | awk '{print $1}'; }

move_one() {
  # $1 = source repo label, $2 = source rel path
  local src_label="$1" src_rel="$2"
  local src_repo dest_repo dest_rel
  if [ "$src_label" = "gathering" ]; then
    src_repo="$GATHERING_REPO"; dest_repo="$CHORUS_REPO"
  else
    src_repo="$CHORUS_REPO"; dest_repo="$GATHERING_REPO"
  fi
  dest_rel=$(map_destination "$src_label" "$src_rel")
  local src_abs="$src_repo/$src_rel"
  local dest_abs="$dest_repo/$dest_rel"

  if [ ! -e "$src_abs" ]; then
    echo "  SKIP: $src_label:$src_rel — source missing" >&2
    return 0
  fi

  if [ "$DRY" = "1" ]; then
    echo "  DRY: $src_label:$src_rel → $(basename "$dest_repo"):$dest_rel"
    return 0
  fi

  mkdir -p "$(dirname "$dest_abs")"

  if [ -e "$dest_abs" ]; then
    # Collision
    if [ "$(sha "$src_abs")" = "$(sha "$dest_abs")" ]; then
      (cd "$src_repo" && git rm -q "$src_rel" 2>/dev/null) || rm "$src_abs"
      echo "  DEDUP: $src_label:$src_rel — identical to destination, removed source"
      return 0
    fi
    # Different content — compare mtime. If destination is newer, treat it as canonical
    # (common case: a prior move left stale fork in source repo). Drop source as stale.
    local src_ts dest_ts
    src_ts=$(stat -f '%m' "$src_abs" 2>/dev/null || echo 0)
    dest_ts=$(stat -f '%m' "$dest_abs" 2>/dev/null || echo 0)
    if [ "$dest_ts" -ge "$src_ts" ] 2>/dev/null; then
      (cd "$src_repo" && git rm -q "$src_rel" 2>/dev/null) || rm "$src_abs"
      echo "  STALE: $src_label:$src_rel — destination is newer, removed source as stale fork"
      return 0
    fi
    # Source is newer than destination — rename source with -src suffix for human resolution (#2461)
    local stem ext resolved
    stem="${dest_rel%.*}"; ext="${dest_rel##*.}"
    resolved="${stem}-from-${src_label}-src.${ext}"
    dest_abs="$dest_repo/$resolved"
    dest_rel="$resolved"
    echo "  COLLIDE: renamed source → $dest_rel (destination preserved, source newer — #2461 to resolve)"
  fi

  # Cross-repo move: copy to destination repo, add, then git rm in source
  cp "$src_abs" "$dest_abs"
  (cd "$dest_repo" && git add "$dest_rel" 2>/dev/null) || true
  (cd "$src_repo" && git rm -q "$src_rel" 2>/dev/null) || rm "$src_abs"
  echo "  MOVE: $src_label:$src_rel → $(basename "$dest_repo"):$dest_rel"
}

if [ ! -f "$INPUT" ]; then
  echo "ERROR: input TSV not found: $INPUT" >&2
  exit 1
fi

moves=0; dedups=0; collides=0; skips=0
while IFS=$'\t' read -r repo path state cabinet owner catalog topic; do
  [ "$state" = "wrong-cabinet" ] || continue
  out=$(move_one "$repo" "$path" 2>&1)
  echo "$out"
  case "$out" in
    *DEDUP:*)    dedups=$((dedups+1)) ;;
    *COLLIDE:*)  collides=$((collides+1)); moves=$((moves+1)) ;;
    *MOVE:*)     moves=$((moves+1)) ;;
    *SKIP:*)     skips=$((skips+1)) ;;
  esac
done < "$INPUT"

echo ""
echo "Summary: $moves moved ($collides with collisions), $dedups deduped, $skips skipped${DRY:+ (dry-run)}"
