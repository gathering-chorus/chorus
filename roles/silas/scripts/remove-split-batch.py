#!/usr/bin/env python3
"""Remove SplitRecord — let full array go to SSH in one call."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

def call(method, path, data=None):
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data:
        with open("/tmp/nifi-payload.json", "w") as f: json.dump(data, f)
        args.extend(["-d", "@/tmp/nifi-payload.json"])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Find processor IDs
procs = call("GET", f"process-groups/{PG}/processors")
pm = {p["component"]["name"]: p["id"] for p in procs.get("processors", [])}
convert_id = pm.get("2. Avro to JSON")
split_id = pm.get("2b. Split Records")
enrich_id = pm.get("3. Enrich + N-Triples (SSH Library)")

# Drop all queues
conns = call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
time.sleep(2)

# Delete connections to/from Split
for conn in conns.get("connections", []):
    src = conn["component"]["source"]["id"]
    dst = conn["component"]["destination"]["id"]
    if src == split_id or dst == split_id:
        crev = conn["revision"]["version"]
        call("DELETE", f"connections/{conn['id']}?version={crev}")
        time.sleep(0.5)
print("Deleted Split connections")

# Delete Split processor
sp = call("GET", f"processors/{split_id}")
call("DELETE", f"processors/{split_id}?version={sp['revision']['version']}")
print("Deleted SplitRecord")

# Connect Convert → Enrich directly
call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": convert_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": enrich_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "Convert→Enrich"
    }
})
print("Connected Convert→Enrich")

call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("RUNNING")
