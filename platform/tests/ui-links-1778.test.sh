#!/bin/bash
# @test-type: integration:ui — needs-stack: every link/asset a Jeff-facing page
# serves must resolve. Skip-if-absent when the stack is down.
#
# #1778 round 2 (pair kade+wren) — Jeff: fold asserts passed while 13 links
# 404'd, a stylesheet among them. Rules, agreed in pair:
#   * template-string hrefs (' + / ${) are code, not links — filtered
#   * 401/403 PASS — an auth wall is a live answer (July's misread, never again)
#   * BASELINE RATCHET keyed (page → target): today's known-dead are pinned,
#     any NEW dead link fails tonight; the fix card burns the pin to zero
#   * DOWN-ONLY: a baselined pair that returns 2xx/3xx/401 LEAVES the baseline
#     in the same run — an entry can never excuse a fixed-then-rebroken link
set -u
API="${UI_SEAMS_API:-http://localhost:3340}"
HERE="$(cd "$(dirname "$0")" && pwd)"
BASELINE="${UI_LINKS_BASELINE:-$HERE/fixtures/link-baseline-1778.tsv}"
PAGES="${UI_LINKS_PAGES:-/chorus /werk /flow /loom /borg-assessment /chorus-model-data /model-data /harvesting/icd}"
PASS=0; FAIL=0; BASELINED=0; HEALED=0

if ! curl -sf --max-time 4 "$API/api/chorus/health" >/dev/null 2>&1; then
  echo "SKIP: chorus-api absent at $API — needs-stack suite not run"; exit 0
fi

in_baseline() { grep -qF "$1	$2" "$BASELINE" 2>/dev/null; }
NEW_BASELINE="$(mktemp)"; trap 'rm -f "$NEW_BASELINE"' EXIT

check_target() {  # page target
  local page="$1" t="$2" code
  case "$t" in
    ''|'#'*|mailto:*|javascript:*) return ;;
    *"' +"*|*'${'*|*"' + "*) return ;;              # template strings are code
    http*) url="$t" ;;
    /*) url="$API$t" ;;
    *) url="$API/${t}" ;;
  esac
  case "$url" in "$API"*) ;; *) return ;; esac       # same-origin only
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 6 "$url")
  case "$code" in
    2*|3*|401|403)
      if in_baseline "$page" "$t"; then
        HEALED=$((HEALED+1))
        echo "HEALED: $page -> $t ($code) — leaving baseline (down-only)"
      else
        PASS=$((PASS+1))
      fi ;;
    *)
      if in_baseline "$page" "$t"; then
        BASELINED=$((BASELINED+1))
        printf '%s\t%s\n' "$page" "$t" >> "$NEW_BASELINE"   # still dead: keep pin
        echo "BASELINE: $page -> $t ($code) — known dead, fix card owns it"
      else
        FAIL=$((FAIL+1))
        echo "FAIL: NEW dead link $page -> $t ($code)"
      fi ;;
  esac
}

for page in $PAGES; do
  html=$(curl -sL --max-time 8 "$API$page")
  [ -n "$html" ] || { echo "FAIL: $page served nothing"; FAIL=$((FAIL+1)); continue; }
  printf '%s' "$html" \
    | grep -oE '(href|src)="[^"]+"' | cut -d'"' -f2 | sort -u \
    | while IFS= read -r t; do check_target "$page" "$t"; done
done > "$NEW_BASELINE.report" 2>&1
# the while ran in a subshell; recount from the report (portable, no lastpipe)
PASS=$(grep -ac '^PASS' "$NEW_BASELINE.report"); PASS=${PASS:-0}
FAIL=$(grep -ac '^FAIL' "$NEW_BASELINE.report"); FAIL=${FAIL:-0}
BASELINED=$(grep -ac '^BASELINE' "$NEW_BASELINE.report"); BASELINED=${BASELINED:-0}
HEALED=$(grep -ac '^HEALED' "$NEW_BASELINE.report"); HEALED=${HEALED:-0}
grep -aE '^(FAIL|BASELINE|HEALED)' "$NEW_BASELINE.report" || true

# down-only shrink: healed pairs are gone from NEW_BASELINE by construction
if [ "$HEALED" -gt 0 ] && [ -w "$BASELINE" ]; then
  grep -aE '^BASELINE' "$NEW_BASELINE.report" | sed 's/^BASELINE: //; s/ -> /\t/; s/ (.*//' > "$BASELINE"
  echo "baseline shrunk by $HEALED healed link(s)"
fi

# NEGATIVE PROOF (#3734): a page carrying a NEW dead href must FAIL.
neg=$(check_target "/negative-fixture" "/definitely-dead-route-1778" 2>&1)
if echo "$neg" | grep -q '^FAIL'; then
  echo "PASS: negative proof — a new dead link fails"
else
  echo "FAIL: negative proof — new dead link did not fail: $neg"; FAIL=$((FAIL+1))
fi

echo "=== ui-links: new-dead=$FAIL baselined=$BASELINED healed=$HEALED ==="
[ "$FAIL" -eq 0 ]
