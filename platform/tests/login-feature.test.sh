#!/usr/bin/env bash
# @test-type: integration — runs the bats cases named from each login.feature scenario, on the fixture harness; no prod
# @domain: identity
#
# #4367 — the login scenarios Jeff reads, each run by the bats cases named from it:
# `@test "login: <scenario title>"` (a further case adds " — <what it checks>").
# RED, by name: a scenario without Given, When and Then steps; a scenario no case is
# named from (so renaming a scenario reds it); a failing case; a case named
# "login: …" that matches no scenario; a scenario waiting on a card — a held test
# is a red with another word (Jeff). Prints one line per scenario and a summary, e.g.
#   15 green, 8 red (waiting on #4361 #4362 …)
# Usage: login-feature.test.sh [feature-file] [tests-dir]   (defaults: the real ones)
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FEATURE="${1:-$ROOT/platform/tests/features/login.feature}"
TESTS="${2:-$ROOT/platform/tests}"
[ -f "$FEATURE" ] || { echo "RED: no feature file at $FEATURE"; exit 1; }
RES="$(mktemp)"; OUT="$(mktemp)"; trap 'rm -f "$RES" "$OUT"' EXIT
waits=""
# idx <TAB> kind <TAB> scenario <TAB> detail   (kind: case | waiting | nocase | steps | orphan)
while IFS=$'\t' read -r idx kind scenario detail; do
  case "$kind" in
    steps)   echo "RED    $scenario — no $detail step"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES" ;;
    waiting) echo "RED    $scenario — waiting on $detail"; echo "$idx red" >> "$RES"; waits="$waits $detail" ;;
    nocase)  echo "RED    $scenario — no case is named \"login: $scenario\""; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES" ;;
    orphan)  echo "RED    case \"$scenario\" in $detail names no scenario"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES" ;;
    case)
      file="${detail%% :: *}"; title="${detail#* :: }"
      # bats -f is a regex: match the exact title
      re="^$(printf '%s' "$title" | sed 's/[][\.*^$+?(){}|/]/\\&/g')\$"
      if bats -f "$re" "$TESTS/$file" </dev/null >"$OUT" 2>&1 && grep -q '^ok 1 ' "$OUT" && ! grep -q '^not ok' "$OUT"; then
        echo "green  $scenario"; echo "$idx green" >> "$RES"
      else
        echo "RED    $scenario — $file :: $title"; printf '%s red\n%s broken\n' "$idx" "$idx" >> "$RES"
        grep -E '^#|not ok' "$OUT" | head -4 | sed 's/^/         /'
      fi ;;
  esac
done < <(python3 - "$FEATURE" "$TESTS" <<'PY'
import sys,re,glob,os
feature,tests=sys.argv[1],sys.argv[2]
scen=[]  # [title, steps set, waiting list]
for line in open(feature):
    m=re.match(r'\s*Scenario:\s*(.+)',line)
    if m: scen.append([m.group(1).strip(),set(),[]]); continue
    if not scen: continue
    m=re.match(r'\s*(Given|When|Then)\b',line)
    if m: scen[-1][1].add(m.group(1)); continue
    m=re.match(r'\s*#\s*waiting on:\s*(#\d+)',line)
    if m: scen[-1][2].append(m.group(1))
cases=[]  # (file, full title, scenario part)
for p in sorted(glob.glob(os.path.join(tests,'*.bats'))):
    for m in re.finditer(r'^@test "login: (.+?)"',open(p).read(),re.M):
        full=m.group(1); cases.append((os.path.basename(p),'login: '+full,full.split(' — ')[0]))
titles=[s[0] for s in scen]
for i,(t,steps,waits) in enumerate(scen,1):
    for k in ('Given','When','Then'):
        if k not in steps: print(f"{i}\tsteps\t{t}\t{k}")
    for w in waits: print(f"{i}\twaiting\t{t}\t{w}")
    mine=[c for c in cases if c[2]==t]
    if not mine and not waits: print(f"{i}\tnocase\t{t}\t-")
    for f,full,_ in mine: print(f"{i}\tcase\t{t}\t{f} :: {full}")
for j,(f,full,part) in enumerate(cases):
    if part not in titles: print(f"o{j}\torphan\t{full}\t{f}")
PY
)
# a scenario is green only when every case that proves it is green
red=$(awk '$2=="red"{print $1}' "$RES" | sort -u | wc -l | tr -d ' ')
green=$(awk '$2!="broken"{print $1}' "$RES" | sort -u | wc -l | tr -d ' '); green=$((green - red))
summary="$green green, $red red"
[ -n "$waits" ] && summary="$summary (waiting on$(printf '%s\n' $waits | sort -u | tr '\n' ' ' | sed 's/ $//; s/^/ /'))"
echo "=== $summary ==="
# Exit: 1 when a scenario is red because THIS tree fails it (missing steps, no
# case, a failing case, an orphan case). A scenario waiting on another card is red
# in the report above, by name and card, every run — but it does not block an
# unrelated card's pipeline.
broken=$(awk '$2=="broken"{print $1}' "$RES" | sort -u | wc -l | tr -d ' ')
[ "$broken" -eq 0 ]
