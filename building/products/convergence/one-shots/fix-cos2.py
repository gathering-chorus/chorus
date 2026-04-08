#!/usr/bin/env python3
import json, subprocess, time
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Stop PG
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json",
    f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "STOPPED"})], capture_output=True)
time.sleep(3)

# Get current state
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
d = json.loads(r.stdout)
rev = d["revision"]["version"]
print(f"State: {d['component']['state']}, COS: {d['component']['config']['properties']['Content Output Strategy']}, rev: {rev}")

# Update
payload = json.dumps({"revision": {"version": rev}, "component": {"id": PROC, "config": {"properties": {"Content Output Strategy": "ORIGINAL"}}}})
r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{PROC}", "-d", payload], capture_output=True, text=True)
if r2.stdout.strip():
    d2 = json.loads(r2.stdout)
    print(f"Updated: COS={d2['component']['config']['properties']['Content Output Strategy']}, valid={not d2['component'].get('validationErrors')}")
else:
    print(f"Empty response. stderr: {r2.stderr[:200]}")

# Start PG
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json",
    f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "RUNNING"})], capture_output=True)
time.sleep(15)

# Check
r3 = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/flow/process-groups/{PG}/status"], capture_output=True, text=True)
s = json.loads(r3.stdout)["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
