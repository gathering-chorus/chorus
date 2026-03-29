#!/usr/bin/env python3
"""Trigger the Apple Extract processor to run once."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

def nifi_call(method, path, data=None):
    token_r = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True)
    token = token_r.stdout.strip()
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

# Find extract processor
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    if "Extract" in p["component"]["name"]:
        pid = p["id"]
        state = p["component"]["state"]
        sched = p["component"]["config"]["schedulingPeriod"]
        props = p["component"]["config"]["properties"]
        sql_query = props.get("SQL Query") or props.get("SQL select query") or "NOT SET"
        print(f"Processor: {p['component']['name']}")
        print(f"  State: {state}")
        print(f"  Schedule: {sched}")
        print(f"  SQL Query: {sql_query[:80]}")
        print(f"  All props: {list(props.keys())}")

        # Change schedule to 0 sec (run immediately when flowfile available or timer fires)
        rev = p["revision"]["version"]
        result = nifi_call("PUT", f"processors/{pid}", {
            "revision": {"version": rev},
            "component": {
                "id": pid,
                "config": {"schedulingPeriod": "10 sec"}
            }
        })
        print(f"  Changed schedule to 10 sec")
        break

# Check flow after a few seconds
time.sleep(15)
status = nifi_call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"\nFlow: In={s['flowFilesIn']} Queued={s['flowFilesQueued']} Out={s['flowFilesOut']} Threads={s['activeThreadCount']}")
