#!/usr/bin/env bash
# @test-type: integration — runs the bats cases each login.feature scenario names, on the fixture harness; no prod
# @domain: identity
#
# #4367 — the login scenarios Jeff reads, each run by the bats case that proves it.
# A scenario with no case, a case that is not found exactly once, or a scenario
# waiting on a card is RED, by name: a held test is a red with another word (Jeff).
# Prints one line per scenario and a summary, e.g.
#   15 green, 4 red (waiting on #4361 #4362 #4368 #4378)
# Usage: login-feature.test.sh [feature-file] [tests-dir]   (defaults: the real ones)
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FEATURE="${1:-$ROOT/platform/tests/features/login.feature}"
TESTS="${2:-$ROOT/platform/tests}"
[ -f "$FEATURE" ] || { echo "RED: no feature file at $FEATURE"; exit 1; }
RES="$(mktemp)"; trap 'rm -f "$RES" /tmp/login-feature.$$' EXIT
waits=""
while IFS=$'\t' read -r idx kind scenario ref; do
  case "$kind" in
    waiting) echo "RED    $scenario — waiting on $ref"; echo "$idx red" >> "$RES"; waits="$waits $ref" ;;
    none)    echo "RED    $scenario — no case proves it"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES" ;;
    proven)
      file="${ref%% :: *}"; title="${ref#* :: }"
      n=$(grep -cF "@test \"$title\"" "$TESTS/$file" 2>/dev/null || true)
      if [ "${n:-0}" != 1 ]; then echo "RED    $scenario — case not found once in $file: $title"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES"; continue; fi
      # bats -f is a regex: match the exact title
      re="^$(printf '%s' "$title" | sed 's/[][\.*^$+?(){}|/]/\\&/g')\$"
      if bats -f "$re" "$TESTS/$file" </dev/null >/tmp/login-feature.$$ 2>&1 && grep -q '^ok 1 ' /tmp/login-feature.$$ && ! grep -q '^not ok' /tmp/login-feature.$$; then
        echo "green  $scenario"; echo "$idx green" >> "$RES"
      else
        echo "RED    $scenario — $file :: $title"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES"
        grep -E '^#|not ok' /tmp/login-feature.$$ | head -4 | sed 's/^/         /'
      fi ;;
  esac
done < <(python3 - "$FEATURE" <<'PY'
import sys,re
cur=None; refs=[]
idx=0
def flush():
    if cur is None: return
    if not refs: print(f"{idx}\tnone\t{cur}\t-")
    for k,r in refs: print(f"{idx}\t{k}\t{cur}\t{r}")
for line in open(sys.argv[1]):
    m=re.match(r'\s*Scenario:\s*(.+)',line)
    if m: flush(); idx+=1; cur=m.group(1).strip(); refs=[]; continue
    m=re.match(r'\s*#\s*proven by:\s*(.+)',line)
    if m and cur: refs.append(("proven",m.group(1).strip())); continue
    m=re.match(r'\s*#\s*waiting on:\s*(#\d+)',line)
    if m and cur: refs.append(("waiting",m.group(1)))
flush()
PY
)
# a scenario is green only when every case that proves it is green
red=$(awk '$2=="red"{print $1}' "$RES" | sort -u | wc -l | tr -d ' ')
green=$(awk '$2!="broken"{print $1}' "$RES" | sort -u | wc -l | tr -d ' '); green=$((green - red))
summary="$green green, $red red"
[ -n "$waits" ] && summary="$summary (waiting on$(printf '%s\n' $waits | sort -u | tr '\n' ' ' | sed 's/ $//; s/^/ /'))"
echo "=== $summary ==="
# Exit: 1 when a scenario is red because THIS tree fails it (a failing, missing or
# absent case). A scenario waiting on another card is red in the report above, by
# name and card, every run — but it does not block an unrelated card's pipeline.
broken=$(awk '$2=="broken"{print $1}' "$RES" | sort -u | wc -l | tr -d ' ')
[ "$broken" -eq 0 ]
