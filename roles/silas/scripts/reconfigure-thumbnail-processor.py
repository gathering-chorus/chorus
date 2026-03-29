#!/usr/bin/env python3
"""Reconfigure Thumbnail Enrichment to use ExecuteStreamCommand via SSH to Library."""
import json, subprocess

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
THUMB = "29baf702-019d-1000-3c77-99381f3496ea"

def nifi_call(method, path, data=None):
    token_r = subprocess.run(
        ["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"],
        capture_output=True, text=True)
    token = token_r.stdout.strip()
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}",
            "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try:
        return json.loads(r.stdout) if r.stdout.strip() else {}
    except json.JSONDecodeError:
        return {}

# First stop the PG to modify processors
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
import time; time.sleep(3)

# Delete the old ExecuteScript thumbnail processor
proc = nifi_call("GET", f"processors/{THUMB}")
rev = proc["revision"]["version"]

# We can't change processor type — need to delete and recreate as ExecuteStreamCommand
# First disconnect it
# Get connections
pg_flow = nifi_call("GET", f"process-groups/{PG}/connections")
connections = pg_flow.get("connections", [])

# Delete connections to/from THUMB
for conn in connections:
    src = conn["component"]["source"]["id"]
    dst = conn["component"]["destination"]["id"]
    if src == THUMB or dst == THUMB:
        crev = conn["revision"]["version"]
        nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
        print(f"Deleted connection {conn['id'][:12]}")
        time.sleep(0.5)

# Delete old processor
nifi_call("DELETE", f"processors/{THUMB}?version={rev}")
print("Deleted old thumbnail processor")

# Create new ExecuteStreamCommand processor
result = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "1d. Enrich — Thumbnail via SSH to Library",
        "position": {"x": 400, "y": 300},
        "config": {
            "properties": {
                "Command Path": "/usr/bin/ssh",
                "Command Arguments": "192.168.86.36 python3 /Users/jeffbridwell/CascadeProjects/architect/scripts/generate-thumbnails-library.py",
                "Working Directory": "/tmp",
                "Argument Delimiter": " ",
                "Output Destination Attribute": "",
                "Ignore STDIN": "false"
            },
            "autoTerminatedRelationships": ["nonzero status"]
        }
    }
})
new_thumb_id = result["id"]
print(f"Created new processor: {new_thumb_id[:12]}")

# Recreate connections
APPLE = "29baf61e-019d-1000-d44c-1ab923817682"
IPHONE = "29baf63e-019d-1000-57a7-33734e4630c3"
TAKEOUT = "29baf65b-019d-1000-1035-ffccbeda7303"
SHACL = "29baf722-019d-1000-03a6-d90719b1f108"

for src, name in [(APPLE, "Apple→Thumb"), (IPHONE, "iPhone→Thumb"), (TAKEOUT, "Takeout→Thumb")]:
    nifi_call("POST", f"process-groups/{PG}/connections", {
        "revision": {"version": 0},
        "component": {
            "source": {"id": src, "groupId": PG, "type": "PROCESSOR"},
            "destination": {"id": new_thumb_id, "groupId": PG, "type": "PROCESSOR"},
            "selectedRelationships": ["success"],
            "name": name
        }
    })
    print(f"Connected {name}")

# Thumb → SHACL
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": new_thumb_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": SHACL, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["output stream"],
        "name": "Thumb→SHACL"
    }
})
print("Connected Thumb→SHACL")

# Restart PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("Pipeline restarted")
