#!/usr/bin/env bash
# gemba-tick.sh — Delta-mode gemba tick (#2194).
# Reports what changed since the previous tick. Silent when nothing changed.
# Sources: pulse (state+card+WIP), tiles API (lastAction), git (HEAD+diff).
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ROLE="${1:?Usage: gemba-tick.sh <role>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PULSE_FILE="${PULSE_FILE:-/tmp/pulse-latest.json}"
TILES_API="${TILES_API:-http://localhost:3470/api/tiles}"
EPOCH_FILE="/tmp/gemba-start-epoch-${ROLE}"
SNAPSHOT_FILE="/tmp/gemba-snapshot-${ROLE}.json"
NOW=$(date +%s)
NOW_HHMM=$(TZ=America/New_York date '+%H:%M')

if [ ! -f "$EPOCH_FILE" ]; then
  echo "$NOW" > "$EPOCH_FILE"
fi
START_EPOCH=$(cat "$EPOCH_FILE")
ELAPSED=$(( NOW - START_EPOCH ))

case "$ROLE" in
  silas) ROLE_DIR="${CHORUS_ROOT}/roles/silas" ;;
  kade)  ROLE_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site" ;;
  wren)  ROLE_DIR="${CHORUS_ROOT}/roles/wren" ;;
  *)     ROLE_DIR="" ;;
esac

TILE_JSON=$(curl -sf --max-time 2 "$TILES_API" 2>/dev/null || echo "[]")
PULSE_JSON="{}"
[ -f "$PULSE_FILE" ] && PULSE_JSON=$(cat "$PULSE_FILE")

export ROLE SNAPSHOT_FILE PULSE_JSON TILE_JSON NOW_HHMM ELAPSED CHORUS_ROOT ROLE_DIR SCRIPT_DIR

python3 <<'PY'
import json, os, re, subprocess

role = os.environ['ROLE']
snap_file = os.environ['SNAPSHOT_FILE']
now_hhmm = os.environ['NOW_HHMM']
elapsed = int(os.environ['ELAPSED'])
chorus_root = os.environ['CHORUS_ROOT']
role_dir = os.environ.get('ROLE_DIR','')
script_dir = os.environ['SCRIPT_DIR']

try: pulse = json.loads(os.environ['PULSE_JSON'])
except Exception: pulse = {}
try: tiles = json.loads(os.environ['TILE_JSON'])
except Exception: tiles = []

def git_snap(d):
    if not d or not os.path.isdir(os.path.join(d, '.git')): return None
    def run(args):
        try: return subprocess.check_output(['git','-C',d]+args, stderr=subprocess.DEVNULL).decode().strip()
        except Exception: return ''
    return {
        'dir':   os.path.basename(d),
        'sha':   run(['rev-parse','HEAD']),
        'msg':   run(['log','-1','--pretty=%s']),
        'files': sorted(run(['diff','--name-only']).splitlines()),
    }

git_state = {}
for d in [chorus_root, role_dir]:
    g = git_snap(d)
    if g: git_state[g['dir']] = g

r = pulse.get('roles', {}).get(role, {})
wip = sorted([str(c.get('id')) for c in pulse.get('board', {}).get('wip_cards', [])
              if (c.get('owner') or '').lower() == role.lower()])

tile = next((t for t in tiles if t.get('role') == role), {})
action_raw = tile.get('lastAction','') or ''
action_age = tile.get('lastActionAge','') or ''

def categorize(a):
    al = a.lower()
    # #2193 AC4: observer.digest lines and raw bash echoes are not
    # meaningful work events — filter them to 'noise' so gemba-tick says
    # 'silent' instead of surfacing shell strings Jeff has to decode.
    if 'observer.digest' in al or '"event":"observer.digest"' in al: return 'noise'
    # bash: prefix with no subsequent verb-like content is typically a digest echo
    if al.startswith('bash:') and not re.search(r'\b(git|npm|cargo|curl|bats|jest|npx|node|python)\b', al):
        return 'noise'
    if any(x in al for x in ['gemba-tick','gemba-start','role-screenshot']): return 'self'
    if 'git commit' in al: return 'commit'
    if re.search(r'\b(jest|vitest|npm (run )?test|cargo test)\b', al): return 'test'
    if 'nudge ' in al or '/nudge' in al: return 'nudge'
    if re.search(r'\bcards (move|done|add|update|reassign|reject|block)\b', al): return 'board'
    if 'curl' in al or 'chorus/search' in al or '/api/' in al: return 'api'
    if re.search(r'\b(cat|less|head|tail|ls|find|grep|rg) ', al): return 'read'
    if re.search(r'\.(ts|js|tsx|jsx|rs|py|sh|md|json|ttl)\b', al): return 'edit'
    return 'other'

cat = categorize(action_raw)

current = {
    'state':       r.get('state',''),
    'card':        r.get('card',''),
    'wip':         wip,
    # #2193 AC4: 'self' and 'noise' both suppress action surface — gemba
    # reports silence rather than echoing raw shell strings.
    'action_key':  '' if cat in ('self', 'noise') else action_raw,
    'action':      '' if cat in ('self', 'noise') else action_raw[:140],
    'cat':         cat,
    'git':         git_state,
}

try: prev = json.load(open(snap_file))
except Exception: prev = {}

deltas = []

if prev.get('state') and prev['state'] != current['state']:
    deltas.append(f"state: {prev['state']} -> {current['state']}")

if prev.get('card','') != current['card']:
    po = prev.get('card') or '-'
    pn = current['card'] or '-'
    if po != '-' or pn != '-':
        deltas.append(f"card: {po} -> {pn}")

prev_wip = set(prev.get('wip', []))
cur_wip  = set(wip)
for c in sorted(cur_wip - prev_wip): deltas.append(f"WIP added: #{c}")
for c in sorted(prev_wip - cur_wip): deltas.append(f"WIP removed: #{c}")

for d, cur_g in current['git'].items():
    prev_g = prev.get('git', {}).get(d, {})
    if prev_g.get('sha') and prev_g['sha'] != cur_g['sha']:
        deltas.append(f"commit {d}: {cur_g['sha'][:8]} {cur_g['msg'][:80]}")
    pf = set(prev_g.get('files', []))
    cf = set(cur_g.get('files', []))
    for f in sorted(cf - pf): deltas.append(f"file+ {d}: {f}")
    for f in sorted(pf - cf): deltas.append(f"file- {d}: {f}")

if current['cat'] not in ('self', 'noise') and current['action_key'] and prev.get('action_key','') != current['action_key']:
    deltas.append(f"{current['cat']} ({action_age}): {current['action']}")

emit_screenshot = bool(deltas)

if deltas:
    print(f"[{now_hhmm}] {role} elapsed={elapsed}s")
    for line in deltas:
        print(f"  {line}")
else:
    last = prev.get('last_emit', 'start')
    print(f"[{now_hhmm}] {role}: no change since {last}")

if emit_screenshot:
    try:
        sp = subprocess.check_output([os.path.join(script_dir,'role-screenshot.sh'), role],
                                     stderr=subprocess.DEVNULL, timeout=5).decode().strip()
        if sp and os.path.isfile(sp):
            print(f"  screen: {sp}")
    except Exception:
        pass

current['last_emit'] = now_hhmm
json.dump(current, open(snap_file,'w'), indent=2)

if elapsed > 600:
    print("## TTL EXPIRED")
    print(f"Observation window: {elapsed}s (limit: 600s)")
PY
