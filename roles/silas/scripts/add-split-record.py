#!/usr/bin/env python3
"""Add SplitRecord between ConvertRecord and ExecuteStreamCommand."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

def call(method, path, data=None):
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data:
        f = "/tmp/nifi-payload.json"
        with open(f, "w") as fh: json.dump(data, fh)
        args.extend(["-d", f"@{f}"])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

# Stop
call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Find processor IDs
procs = call("GET", f"process-groups/{PG}/processors")
convert_id = enrich_id = None
for p in procs.get("processors", []):
    if "Avro to JSON" in p["component"]["name"]: convert_id = p["id"]
    if "Enrich" in p["component"]["name"]: enrich_id = p["id"]

# Find JSON Reader and Writer service IDs
css = call("GET", f"flow/process-groups/{PG}/controller-services")
reader_id = writer_id = None
for cs in css.get("controllerServices", []):
    if cs["component"]["name"] == "JSON Writer" and cs["component"]["state"] == "ENABLED": writer_id = cs["id"]

# Need a JsonTreeReader
reader = call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {"type": "org.apache.nifi.json.JsonTreeReader", "name": "JSON Reader"}
})
reader_id = reader["id"]
call("PUT", f"controller-services/{reader_id}/run-status", {"revision": reader["revision"], "state": "ENABLED"})
time.sleep(2)
print(f"JSON Reader: {reader_id[:12]}")

# Delete Convert→Enrich connection
conns = call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    if conn["component"]["source"]["id"] == convert_id and conn["component"]["destination"]["id"] == enrich_id:
        call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
        time.sleep(1)
        crev = conn["revision"]["version"]
        call("DELETE", f"connections/{conn['id']}?version={crev}")
        print("Deleted Convert→Enrich")
        time.sleep(1)
        break

# Create SplitRecord
split = call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.SplitRecord",
        "name": "2b. Split Records",
        "position": {"x": 400, "y": 300},
        "config": {
            "properties": {
                "Record Reader": reader_id,
                "Record Writer": writer_id,
                "Records Per Split": "1"
            },
            "autoTerminatedRelationships": ["failure", "original"]
        }
    }
})
split_id = split["id"]
print(f"SplitRecord: {split_id[:12]}")

# Connect Convert → Split
call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": convert_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": split_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "Convert→Split"
    }
})

# Connect Split → Enrich
call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": split_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": enrich_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["splits"],
        "name": "Split→Enrich"
    }
})
print("Connections created")

# Start
call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
time.sleep(15)

status = call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: Queued={s['flowFilesQueued']} Threads={s['activeThreadCount']}")
