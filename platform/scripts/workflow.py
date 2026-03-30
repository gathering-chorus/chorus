#!/usr/bin/env python3
"""workflow.py — Workflow execution engine (extracted from workflow.sh for #1624)"""
import json, sys, os, glob, subprocess
from datetime import datetime, timezone

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(SCRIPT_DIR))
WORKFLOWS_DIR = os.path.join(REPO_ROOT, "messages", "workflows")
ACTIVE_DIR = os.path.join(WORKFLOWS_DIR, "active")
ARCHIVE_DIR = os.path.join(WORKFLOWS_DIR, "archive")
BOARD_TS = os.path.join(SCRIPT_DIR, "cards")

os.makedirs(ACTIVE_DIR, exist_ok=True)
os.makedirs(ARCHIVE_DIR, exist_ok=True)

def next_id():
    mx = 0
    for d in [ACTIVE_DIR, ARCHIVE_DIR]:
        for f in glob.glob(os.path.join(d, "WF-*.json")):
            try:
                num = int(os.path.basename(f).replace("WF-","").replace(".json",""))
                if num > mx: mx = num
            except ValueError: pass
    return f"WF-{mx+1:03d}"

def now_iso():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def load_wf(wf_id):
    for d in [ACTIVE_DIR, ARCHIVE_DIR]:
        p = os.path.join(d, f"{wf_id}.json")
        if os.path.exists(p):
            with open(p) as f: return json.load(f), p
    return None, None

def chorus_log(event, role, detail):
    cl = os.path.join(SCRIPT_DIR, "chorus-log.sh")
    if os.path.exists(cl):
        try: subprocess.run([cl, event, role, detail], capture_output=True, timeout=5)
        except: pass

def cmd_create(args):
    decision = args[0]
    source, card, steps_raw = "verbal", "", ""
    i = 1
    while i < len(args):
        if args[i] == "--source": source = args[i+1]; i += 2
        elif args[i] == "--card": card = args[i+1]; i += 2
        elif args[i] == "--steps": steps_raw = args[i+1]; i += 2
        else: print(f"Unknown arg: {args[i]}"); sys.exit(1)
    if not steps_raw: print("Error: --steps required"); sys.exit(1)
    wf_id = next_id()
    now = now_iso()
    steps = []
    for idx, part in enumerate(steps_raw.split(','), 1):
        role, _, action = part.strip().partition(':')
        role, action = role.strip().lower(), action.strip()
        if role not in ('silas','kade','wren','jeff'): print(f"Error: unknown role '{role}'"); sys.exit(1)
        steps.append({"seq":idx,"role":role,"action":action,"status":"ready" if idx==1 else "pending",
                      "card":None,"blocked_by":[idx-1] if idx>1 else [],"artifacts":[],"brief":None,
                      "started_at":None,"completed_at":None,"notes":None})
    wf = {"id":wf_id,"decision":decision,"source":source,"card":int(card) if card else None,
          "created":now,"updated":now,"status":"in_progress","steps":steps,"verification":None,
          "history":[{"timestamp":now,"event":"created","role":"system","detail":f"Workflow created: {decision}"},
                     {"timestamp":now,"event":"step_ready","role":steps[0]["role"],"detail":f"Step 1 ready: {steps[0]['action']}"}]}
    with open(os.path.join(ACTIVE_DIR, f"{wf_id}.json"), 'w') as f: json.dump(wf, f, indent=2)
    print(wf_id)

def cmd_advance(args):
    wf_id = args[0]
    notes, artifacts_raw = "", ""
    i = 1
    while i < len(args):
        if args[i] == "--notes": notes = args[i+1]; i += 2
        elif args[i] == "--artifacts": artifacts_raw = args[i+1]; i += 2
        else: i += 1
    wf, wf_path = load_wf(wf_id)
    if not wf: print(f"Error: workflow {wf_id} not found"); sys.exit(1)
    now = now_iso()
    artifacts = [a.strip() for a in artifacts_raw.split(',') if a.strip()] if artifacts_raw else []
    current = next((s for s in wf['steps'] if s['status'] in ('in_progress','ready')), None)
    if not current: print(f"Error: No active step"); sys.exit(1)
    current['status'] = 'completed'; current['completed_at'] = now
    if notes: current['notes'] = notes
    if artifacts: current['artifacts'] = artifacts
    wf['history'].append({"timestamp":now,"event":"step_completed","role":current['role'],
                          "detail":f"Step {current['seq']} completed: {current['action']}" + (f" — {notes}" if notes else "")})
    nxt = None
    for s in wf['steps']:
        if s['status'] == 'pending' and all(wf['steps'][b-1]['status']=='completed' for b in s.get('blocked_by',[])):
            s['status'] = 'ready'; nxt = s
            wf['history'].append({"timestamp":now,"event":"step_ready","role":s['role'],"detail":f"Step {s['seq']} ready: {s['action']}"})
            break
    all_done = all(s['status'] in ('completed','skipped') for s in wf['steps'])
    if all_done:
        wf['status'] = 'completed'
        wf['history'].append({"timestamp":now,"event":"workflow_completed","role":"system","detail":"All steps completed"})
    wf['updated'] = now
    save_path = wf_path
    if wf['status'] == 'completed':
        archive_path = os.path.join(ARCHIVE_DIR, os.path.basename(wf_path))
        os.rename(wf_path, archive_path); save_path = archive_path
        chorus_log('workflow.manifest.completed', current['role'], f'workflow_id={wf["id"]},card_id={wf.get("card","")},step_count={len(wf["steps"])}')
    # Brief for next step
    if nxt and not all_done:
        briefs_map = {'silas':'architect/briefs','kade':'engineer/briefs','wren':'product-manager/briefs'}
        td = os.path.join(REPO_ROOT, briefs_map.get(nxt['role'],''))
        if os.path.isdir(os.path.dirname(td)):
            os.makedirs(td, exist_ok=True)
            bp = os.path.join(td, f"{datetime.now(timezone.utc).strftime('%Y-%m-%d')}-{wf['id'].lower()}-step{nxt['seq']}.md")
            with open(bp,'w') as bf:
                bf.write(f"# Workflow Handoff: {wf['id']} — Step {nxt['seq']}\n\n**Your action**: {nxt['action']}\n\nPrevious: {current['role']} — {current['action']}\n")
            nxt['brief'] = bp
    with open(save_path, 'w') as f: json.dump(wf, f, indent=2)
    print(f"Step {current['seq']} completed ({current['role']}: {current['action']})")
    if all_done: print("Workflow complete")
    elif nxt: print(f"Step {nxt['seq']} now READY for {nxt['role']}: {nxt['action']}")

def cmd_status(args):
    if args:
        wf, _ = load_wf(args[0])
        if not wf: print(f"Error: {args[0]} not found"); sys.exit(1)
        print(f"{'✅' if wf['status']=='completed' else '🔄'} {wf['id']}: {wf['decision']}")
        print(f"   Status: {wf['status']} | Created: {wf['created'][:10]}")
        for s in wf['steps']:
            icon = {'completed':'✅','ready':'👉','in_progress':'🔄','pending':'⬜'}.get(s['status'],'?')
            print(f"   {icon} Step {s['seq']}: [{s['role']}] {s['action']}")
            if s.get('notes'): print(f"      Notes: {s['notes']}")
    else:
        found = False
        for fp in sorted(glob.glob(os.path.join(ACTIVE_DIR, 'WF-*.json'))):
            found = True
            with open(fp) as f: wf = json.load(f)
            done = sum(1 for s in wf.get('steps',[]) if s.get('status')=='completed')
            cur = next((s for s in wf.get('steps',[]) if s.get('status') in ('ready','in_progress')), None)
            ci = f" → {cur.get('role','?')}: {cur.get('action','?')[:50]}" if cur else ""
            print(f"🔄 {wf.get('id','?')} [{done}/{len(wf.get('steps',[]))}] {wf.get('decision','?')[:60]}{ci}")
        if not found: print("No active workflows")

def cmd_list(args):
    cmd_status([])
    if args and args[0] == "--all":
        print()
        for fp in sorted(glob.glob(os.path.join(ARCHIVE_DIR, 'WF-*.json'))):
            with open(fp) as f: wf = json.load(f)
            print(f"✅ {wf.get('id','?')} [{len(wf.get('steps',[]))}/{len(wf.get('steps',[]))}] {wf.get('decision','?')[:60]}")

def cmd_pending(args):
    role = args[0].lower() if args else ""
    for fp in sorted(glob.glob(os.path.join(ACTIVE_DIR, 'WF-*.json'))):
        with open(fp) as f:
            try: wf = json.load(f)
            except: continue
        for s in wf.get('steps', []):
            if s.get('role') == role and s.get('status') == 'ready':
                print(f"{wf.get('id','?')} step {s.get('seq','?')}/{len(wf.get('steps',[]))}: {s.get('action','?')}")

def cmd_history(args):
    wf, _ = load_wf(args[0])
    if not wf: print(f"Error: {args[0]} not found"); sys.exit(1)
    print(f"{wf['id']} — {wf['decision']}")
    for h in wf.get('history',[]):
        ts = h['timestamp'][:16].replace('T',' ')
        print(f"  {ts} [{h.get('role','?')}] {h.get('detail','')}")

def cmd_visualize(args):
    # This is 400 lines of HTML generation — keep calling the original for now
    print("Visualize: use workflow.sh visualize (HTML gen not yet ported)")

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: workflow.py <create|advance|status|list|pending|history> [args]")
        sys.exit(0)
    cmd = sys.argv[1]
    rest = sys.argv[2:]
    cmds = {'create':cmd_create,'advance':cmd_advance,'status':cmd_status,
            'list':cmd_list,'pending':cmd_pending,'history':cmd_history,'visualize':cmd_visualize}
    if cmd in cmds: cmds[cmd](rest)
    else: print(f"Unknown: {cmd}"); sys.exit(1)
