#!/bin/bash
# Compare two performance baselines — flag degradation
# Usage: perf-compare.sh [date1] [date2]  (defaults: yesterday vs today)

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

LOGDIR="${CHORUS_ROOT}/proving/logs"
DATE1=${1:-$(date -v-1d +%Y-%m-%d)}
DATE2=${2:-$(date +%Y-%m-%d)}

F1="$LOGDIR/perf-baseline-${DATE1}.json"
F2="$LOGDIR/perf-baseline-${DATE2}.json"

if [ ! -f "$F1" ]; then echo "No baseline for $DATE1"; exit 1; fi
if [ ! -f "$F2" ]; then echo "No baseline for $DATE2"; exit 1; fi

python3 -c "
import json, sys

with open('$F1') as f: d1 = json.load(f)
with open('$F2') as f: d2 = json.load(f)

print(f'Compare: {d1[\"timestamp\"]} vs {d2[\"timestamp\"]}')
print()

checks = [
    ('Disk %', d1['disk']['percentUsed'], d2['disk']['percentUsed'], 5, 'higher'),
    ('Memory free %', d1['memory']['freePercent'], d2['memory']['freePercent'], -10, 'lower'),
    ('CPU load', d1['cpu']['loadAvg1m'], d2['cpu']['loadAvg1m'], 2, 'higher'),
    ('Processes', d1['processes']['total'], d2['processes']['total'], 50, 'higher'),
    ('Fuseki latency ms', d1['fuseki']['latencyMs'], d2['fuseki']['latencyMs'], 1000, 'higher'),
    ('NiFi queued', d1['nifi']['totalQueued'], d2['nifi']['totalQueued'], 100, 'higher'),
    ('Errors', d1['errors']['lastHour'], d2['errors']['lastHour'], 5, 'higher'),
]

for name, v1, v2, threshold, direction in checks:
    delta = v2 - v1
    flag = ''
    if direction == 'higher' and delta > threshold: flag = ' ⚠ DEGRADED'
    elif direction == 'lower' and delta < threshold: flag = ' ⚠ DEGRADED'
    print(f'  {name:20s} {v1:>10} → {v2:>10}  (delta: {delta:+})${flag}')
"
