#!/usr/bin/env bash
# frustration-telemetry.sh — #2454
# Read-side aggregation over ~/.chorus/index.db for frustration/relief/precursor vocab.
# Emits JSON (for API/chart) or a human-readable table (for CLI demo).
#
# Usage:
#   frustration-telemetry.sh                   # last 30d, human table
#   frustration-telemetry.sh --json            # last 30d, JSON
#   frustration-telemetry.sh --days 14         # last N days
#   frustration-telemetry.sh --json --days 90  # last 90d, JSON
#
# Output is read-only. This tool never changes model behavior (AC: zero behavior change).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="${CHORUS_FRUSTRATION_VOCAB:-$SCRIPT_DIR/../config/frustration-vocab.json}"
DB="${CHORUS_DB:-$HOME/.chorus/index.db}"

DAYS=30
FORMAT=table

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json)   FORMAT=json; shift ;;
    --days)   DAYS="$2"; shift 2 ;;
    --config) CONFIG="$2"; shift 2 ;;
    --db)     DB="$2"; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

[ -f "$CONFIG" ] || { echo "config not found: $CONFIG" >&2; exit 1; }
[ -f "$DB" ] || { echo "db not found: $DB" >&2; exit 1; }

export DB CONFIG DAYS FORMAT

python3 <<'PY'
import json, os, sqlite3, sys
from collections import defaultdict
from datetime import date, timedelta

db = sqlite3.connect(os.environ['DB'])
cfg = json.load(open(os.environ['CONFIG']))
days = int(os.environ['DAYS'])
fmt = os.environ['FORMAT']

cutoff = (date.today() - timedelta(days=days)).isoformat()

CATEGORIES = {
    'frustration': dict(roles=['jeff'], authors=['user']),
    'relief':      dict(roles=['jeff'], authors=['user']),
    'precursor':   dict(roles=['wren','silas','kade'], authors=['assistant']),
}

result = {}
for cat, meta in CATEGORIES.items():
    patterns = cfg.get(cat, {}).get('patterns', [])
    if not patterns:
        result[cat] = {'total': 0, 'by_day': {}, 'description': cfg.get(cat,{}).get('description','')}
        continue
    like_clauses = " OR ".join(["LOWER(content) LIKE ?"] * len(patterns))
    params = ([f'%{p}%' for p in patterns]
              + meta['authors'] + meta['roles'] + [cutoff])
    role_ph = ','.join('?' for _ in meta['roles'])
    author_ph = ','.join('?' for _ in meta['authors'])
    query = f"""
        SELECT DATE(timestamp) AS d, role, COUNT(*) AS n
        FROM messages
        WHERE source='claude'
          AND COALESCE(is_bridge,0)=0
          AND ({like_clauses})
          AND author IN ({author_ph})
          AND role IN ({role_ph})
          AND timestamp >= ?
        GROUP BY d, role
        ORDER BY d DESC
    """
    by_day = defaultdict(lambda: defaultdict(int))
    total = 0
    for d, role, n in db.execute(query, params):
        by_day[d][role] += n
        total += n
    result[cat] = {
        'total': total,
        'by_day': {d: dict(v) for d, v in by_day.items()},
        'description': cfg.get(cat,{}).get('description',''),
    }

# Team-learning overlay: memory file mtimes grouped by role + day.
# Counts feedback/project memory files across all role memory stores.
from pathlib import Path
import datetime as _dt
mem_by_day = {}
roots = [
    Path.home() / '.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus/memory',
    Path.home() / '.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-wren/memory',
    Path.home() / '.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas/memory',
    Path.home() / '.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-kade/memory',
]
for root in roots:
    if not root.exists(): continue
    for f in root.glob('*.md'):
        if f.name == 'MEMORY.md': continue
        day = _dt.date.fromtimestamp(f.stat().st_mtime).isoformat()
        if day < cutoff: continue
        mem_by_day.setdefault(day, []).append(f.stem)

envelope = {
    'card': '#2454',
    'window_days': days,
    'since': cutoff,
    'generated_at': _dt.datetime.now().isoformat(timespec='seconds'),
    'data': result,
    'memory_writes': {d: len(v) for d, v in mem_by_day.items()},
    'memory_samples': {d: sorted(set(v))[:5] for d, v in mem_by_day.items()},
}

if fmt == 'json':
    print(json.dumps(envelope, indent=2, sort_keys=False))
    sys.exit(0)

# human-readable table
print(f"Frustration Telemetry — last {days} days (since {cutoff})")
print("=" * 72)
for cat in ('frustration', 'relief', 'precursor'):
    d = result[cat]
    total = d['total']
    label = f"[{cat.upper()}]"
    if total == 0:
        print(f"\n{label}  0 detected — honest-fold")
        continue
    print(f"\n{label}  {total} total across {len(d['by_day'])} days")
    for day in sorted(d['by_day'].keys(), reverse=True)[:10]:
        roles = d['by_day'][day]
        by_role = ', '.join(f"{r}={n}" for r, n in sorted(roles.items()))
        daily = sum(roles.values())
        print(f"  {day}  {daily:>4}   ({by_role})")
PY
