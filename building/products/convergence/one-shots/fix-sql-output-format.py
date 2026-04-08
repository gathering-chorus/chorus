#!/usr/bin/env python3
"""
Fix Apple Extract — change output format and reconnect.
ExecuteSQL in NiFi 2.8 outputs Avro by default. Try ExecuteSQLRecord with JSON writer instead.
"""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
APPLE = "29baf61e-019d-1000-d44c-1ab923817682"
THUMB = "2a0307ff-019d-1000-a5e9-52eb137b506b"
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

# Stop PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# First: create a JsonRecordSetWriter controller service
writer = nifi_call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.json.JsonRecordSetWriter",
        "name": "JSON Writer",
        "properties": {
            "Output Grouping": "OUTPUT_ARRAY",
            "Pretty Print JSON": "false"
        }
    }
})
writer_id = writer.get("id", "")
if writer_id:
    print(f"Created JSON Writer: {writer_id[:12]}")
    # Enable it
    wrev = writer["revision"]["version"]
    nifi_call("PUT", f"controller-services/{writer_id}/run-status", {
        "revision": {"version": wrev},
        "state": "ENABLED"
    })
    time.sleep(2)
    print("JSON Writer enabled")
else:
    print(f"Failed: {json.dumps(writer)[:200]}")
    exit(1)

# Delete old Apple processor connections
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    src = conn["component"]["source"]["id"]
    if src == APPLE:
        crev = conn["revision"]["version"]
        # Need to drain queue first
        nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
        print(f"Deleted connection from Apple")
        time.sleep(0.5)

# Delete old Apple processor
proc = nifi_call("GET", f"processors/{APPLE}")
rev = proc["revision"]["version"]
nifi_call("DELETE", f"processors/{APPLE}?version={rev}")
print("Deleted old Apple ExecuteSQL")

# Create ExecuteSQLRecord with JSON output
sql = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename,
CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon,
CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a
JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

new_apple = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQLRecord",
        "name": "1a. Extract Apple Photos (JSON)",
        "position": {"x": 100, "y": 100},
        "config": {
            "properties": {
                "Database Connection Pooling Service": DBCP,
                "SQL Query": sql,
                "Record Writer": writer_id,
                "Max Rows Per FlowFile": "0",
                "Output Batch Size": "0"
            },
            "schedulingPeriod": "0 sec",
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
new_apple_id = new_apple.get("id", "")
if new_apple_id:
    print(f"Created ExecuteSQLRecord: {new_apple_id[:12]}")
else:
    print(f"Failed: {json.dumps(new_apple)[:200]}")
    exit(1)

# Connect new Apple → Thumb
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": new_apple_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": THUMB, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "Apple(JSON)→Thumb"
    }
})
print("Connected Apple→Thumb")

# Start PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("Pipeline restarted")
