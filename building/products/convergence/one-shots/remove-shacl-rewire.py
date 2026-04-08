#!/usr/bin/env python3
"""Remove SHACL processor — validation is now in the SSH script. Wire Thumb→Fuseki directly."""
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

print("Stopping...")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Find processor IDs
procs = nifi_call("GET", f"process-groups/{PG}/processors")
proc_map = {p["component"]["name"]: p["id"] for p in procs.get("processors", [])}
thumb_id = proc_map.get("3. Generate Thumbnail (SSH to Library)")
shacl_id = proc_map.get("3b. SHACL Validate")
fuseki_id = proc_map.get("4. Write to Fuseki")

# Delete connections to/from SHACL
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    src = conn["component"]["source"]["id"]
    dst = conn["component"]["destination"]["id"]
    if src == shacl_id or dst == shacl_id:
        nifi_call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
        time.sleep(1)
        crev = conn["revision"]["version"]
        nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
        time.sleep(0.5)
print("Deleted SHACL connections")

# Delete SHACL processor
proc = nifi_call("GET", f"processors/{shacl_id}")
rev = proc["revision"]["version"]
nifi_call("DELETE", f"processors/{shacl_id}?version={rev}")
print("Deleted SHACL processor")

# Connect Thumb → Fuseki directly
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": thumb_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": fuseki_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["output stream"],
        "name": "Thumb(NT)→Fuseki"
    }
})
print("Connected Thumb→Fuseki")

# Also drop any queued flowfiles in all connections (stale JSON data)
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    nifi_call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
time.sleep(2)
print("Dropped all queued flowfiles")

print("Starting...")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("RUNNING")
