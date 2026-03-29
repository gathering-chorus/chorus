#!/usr/bin/env python3
"""
Clean slate: delete all processors in the PG, recreate with correct config.
Keep it simple: Apple Extract (ExecuteSQL, Avro output) → SplitAvro → SSH Thumbnail → Fuseki.
"""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
DBCP = "29efe61f-019d-1000-6861-7246d90a55fb"

def nifi_call(method, path, data=None):
    token_r = subprocess.run(
        ["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"],
        capture_output=True, text=True)
    token = token_r.stdout.strip()
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}",
            "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

print("=== Stopping pipeline ===")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

print("=== Deleting all connections ===")
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    cid = conn["id"]
    crev = conn["revision"]["version"]
    # Drop queued flowfiles first
    nifi_call("POST", f"flowfile-queues/{cid}/drop-requests")
    time.sleep(1)
    nifi_call("DELETE", f"connections/{cid}?version={crev}")
    print(f"  Deleted connection {cid[:12]}")
    time.sleep(0.5)

print("=== Deleting all processors ===")
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    pid = p["id"]
    prev = p["revision"]["version"]
    nifi_call("DELETE", f"processors/{pid}?version={prev}")
    print(f"  Deleted {p['component']['name']}")
    time.sleep(0.5)

print("=== Creating clean pipeline ===")

# 1. Apple Extract — ExecuteSQL with Avro output (default)
sql = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename,
CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon,
CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

apple = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQL",
        "name": "1. Extract Apple Photos",
        "position": {"x": 400, "y": 100},
        "config": {
            "properties": {
                "Database Connection Pooling Service": DBCP,
                "SQL Query": sql
            },
            "autoTerminatedRelationships": ["failure"],
            "schedulingPeriod": "1 day"
        }
    }
})
apple_id = apple["id"]
print(f"  Created Apple Extract: {apple_id[:12]}")

# 2. SSH to Library — runs generate-thumbnails + N-Triples + Fuseki load all in one
# The script reads Avro stdin won't work — instead, have NiFi write the Avro to a temp file
# and the SSH script reads it. Use ExecuteStreamCommand with the Avro flowfile as stdin.
#
# Actually — simpler: have the SSH command run a self-contained script on Library
# that reads from SQLite directly, generates thumbnails, and loads to Fuseki.
# NiFi just triggers it — no data transfer needed.

trigger = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "2. Run Pipeline on Library (SSH)",
        "position": {"x": 400, "y": 300},
        "config": {
            "properties": {
                "Command Path": "/usr/bin/ssh",
                "Command Arguments": "192.168.86.36;python3;/Users/jeffbridwell/CascadeProjects/architect/scripts/nifi-apple-to-fuseki.py",
                "Argument Delimiter": ";",
                "Ignore STDIN": "true"
            },
            "autoTerminatedRelationships": ["output stream", "nonzero status", "original"],
            "schedulingPeriod": "0 sec"
        }
    }
})
trigger_id = trigger["id"]
print(f"  Created SSH Trigger: {trigger_id[:12]}")

# Connect Apple → SSH Trigger
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": apple_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": trigger_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "Extract→Process"
    }
})
print("  Connected Extract→Process")

print("=== Starting pipeline ===")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("=== Done ===")
