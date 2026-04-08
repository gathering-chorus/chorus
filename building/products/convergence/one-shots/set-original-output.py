#!/usr/bin/env python3
import json, subprocess, time
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

def nifi_call(method, path, data=None):
    token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

proc = nifi_call("GET", f"processors/{PROC}")
rev = proc["revision"]["version"]
result = nifi_call("PUT", f"processors/{PROC}", {
    "revision": {"version": rev},
    "component": {"id": PROC, "config": {"properties": {"Content Output Strategy": "ORIGINAL"}}}
})
vs = result.get("component", {}).get("validationErrors", [])
print(f"{'VALID' if not vs else vs}")

nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
time.sleep(15)

status = nifi_call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"In={s['flowFilesIn']} Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
