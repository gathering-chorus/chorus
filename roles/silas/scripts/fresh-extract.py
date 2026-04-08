#!/usr/bin/env python3
"""Delete tangled ExecuteSQL, create fresh one with all correct properties from the start."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
DBCP = "29efe61f-019d-1000-6861-7246d90a55fb"
OLD_PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

SQL = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename, CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken, CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat, CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon, CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height, CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

def call(method, path, data=None):
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

# Stop PG
call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Find and delete connection FROM old extract
conns = call("GET", f"process-groups/{PG}/connections")
convert_id = None
for conn in conns.get("connections", []):
    if conn["component"]["source"]["id"] == OLD_PROC:
        convert_id = conn["component"]["destination"]["id"]
        crev = conn["revision"]["version"]
        call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
        time.sleep(1)
        call("DELETE", f"connections/{conn['id']}?version={crev}")
        print(f"Deleted connection to {convert_id[:12]}")
        time.sleep(1)

# Delete old processor
r = call("GET", f"processors/{OLD_PROC}")
rev = r["revision"]["version"]
call("DELETE", f"processors/{OLD_PROC}?version={rev}")
print("Deleted old ExecuteSQL")

# Create fresh ExecuteSQL — ALL properties set correctly from the start
new = call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQL",
        "name": "1. Extract Apple Photos",
        "position": {"x": 400, "y": 0},
        "config": {
            "properties": {
                "Database Connection Pooling Service": DBCP,
                "SQL select query": SQL,
                "Content Output Strategy": "ORIGINAL"
            },
            "schedulingPeriod": "10 sec",
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
new_id = new.get("id", "")
vs = new.get("component", {}).get("validationErrors", [])
cos = new.get("component", {}).get("config", {}).get("properties", {}).get("Content Output Strategy", "?")
sql_set = new.get("component", {}).get("config", {}).get("properties", {}).get("SQL select query", "NOT SET")[:40]
print(f"Created: {new_id[:12]}, COS={cos}, SQL={sql_set}..., valid={'yes' if not vs else vs}")

# Reconnect to ConvertRecord
if convert_id and new_id:
    call("POST", f"process-groups/{PG}/connections", {
        "revision": {"version": 0},
        "component": {
            "source": {"id": new_id, "groupId": PG, "type": "PROCESSOR"},
            "destination": {"id": convert_id, "groupId": PG, "type": "PROCESSOR"},
            "selectedRelationships": ["success"],
            "name": "Extract→Convert"
        }
    })
    print("Connected Extract→Convert")

# Start PG
call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
time.sleep(15)

# Check
status = call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
