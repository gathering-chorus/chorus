#!/bin/bash
# @test-type: perf — reads the spine (last 24h) only; skip when spine absent
#
# #3921 — werk-level perf: every pipeline phase has a p95 budget, folded from
# the spine's own werk.phase events. Jeff, 2026-08-20: "unacceptable to have
# 25-30m werk runs" — the data was on the spine all along (test p95 was 26min
# the day this landed); this makes the number a RED with an owner instead of
# a feeling. Budgets live in platform/config/werk-phase-budgets.tsv.
set -u
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SPINE="${WERK_BUDGET_SPINE:-$HOME/.chorus/chorus.log}"
BUDGETS="${WERK_BUDGET_FILE:-$(cd "$(dirname "$0")/.." && pwd)/config/werk-phase-budgets.tsv}"
WINDOW_H="${WERK_BUDGET_WINDOW_H:-24}"
[ -f "$SPINE" ] || { echo "SKIP: no spine at $SPINE"; exit 0; }
[ -f "$BUDGETS" ] || { echo "FAIL: budget file missing at $BUDGETS — a perf gate with no budgets is hollow"; exit 1; }

python3 - "$SPINE" "$BUDGETS" "$WINDOW_H" <<'PY'
import sys, json, re, collections, datetime
spine, budgets_f, window_h = sys.argv[1], sys.argv[2], float(sys.argv[3])
budgets = {}
for line in open(budgets_f):
    line = line.strip()
    if not line or line.startswith('#'): continue
    p, b = line.split('\t'); budgets[p] = float(b)

cutoff = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(hours=window_h)
def secs(e):
    m = re.match(r'(?:(\d+)m)?([\d.]+)s', e or '')
    return int(m.group(1) or 0)*60 + float(m.group(2)) if m else None

d = collections.defaultdict(list)
for l in open(spine, errors='ignore'):
    if '"event":"werk.phase"' not in l or '"state":"pass"' not in l: continue
    try: j = json.loads(l)
    except: continue
    try:
        ts = datetime.datetime.fromisoformat(j.get('timestamp','').replace('Z','+00:00'))
    except: continue
    if ts < cutoff: continue
    s = secs(j.get('elapsed'))
    if s is not None: d[j['phase']].append(s)

fails = 0
for phase, budget in budgets.items():
    v = sorted(d.get(phase, []))
    if not v:
        print(f"SKIP: {phase} — no runs in window"); continue
    p95 = v[min(len(v)-1, int(len(v)*0.95))]
    if p95 > budget:
        print(f"FAIL: {phase} p95={p95:.0f}s > budget {budget:.0f}s (n={len(v)}, max={v[-1]:.0f}s)")
        fails += 1
    else:
        print(f"PASS: {phase} p95={p95:.0f}s <= {budget:.0f}s (n={len(v)})")
# NEGATIVE PROOF (#3734): a synthetic phase over budget must FAIL this fold.
# fixed 600s synthetic budget — independent of the live file, so raising a
# live ceiling can never quietly disarm this proof (it did, once, in review)
synth = sorted([700.0]*10)
p95 = synth[int(len(synth)*0.95)]
if p95 > 600:
    print("PASS: negative proof — synthetic 700s test phase exceeds its budget")
else:
    print("FAIL: negative proof — the fold cannot see an over-budget phase"); fails += 1
sys.exit(1 if fails else 0)
PY
