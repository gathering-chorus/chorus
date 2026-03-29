#!/usr/bin/env python3
"""
Fix pipeline using the proven pattern from build-nifi-iphone-native.py (#1652).
The working chain: ExecuteSQL → ConvertRecord(Avro→JSON) → ExecuteStreamCommand(JSON→NT) → InvokeHTTP(PUT Fuseki)
"""
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

# Stop and clean
print("Stopping...")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Drop all queues
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    nifi_call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
time.sleep(2)

# Delete all connections
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    crev = conn["revision"]["version"]
    nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
    time.sleep(0.3)

# Delete all processors
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    prev = p["revision"]["version"]
    nifi_call("DELETE", f"processors/{p['id']}?version={prev}")
    time.sleep(0.3)

# Delete non-DBCP controller services
DBCP = "29efe61f-019d-1000-6861-7246d90a55fb"
css = nifi_call("GET", f"flow/process-groups/{PG}/controller-services")
for cs in css.get("controllerServices", []):
    if cs["id"] == DBCP: continue
    csrev = cs["revision"]["version"]
    if cs["component"]["state"] == "ENABLED":
        nifi_call("PUT", f"controller-services/{cs['id']}/run-status", {"revision": {"version": csrev}, "state": "DISABLED"})
        time.sleep(2)
        cs = nifi_call("GET", f"controller-services/{cs['id']}")
        csrev = cs["revision"]["version"]
    nifi_call("DELETE", f"controller-services/{cs['id']}?version={csrev}")
    time.sleep(0.3)
print("Cleaned")

# === Create services (proven pattern from #1652) ===

# AvroReader
avro_reader = nifi_call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {"type": "org.apache.nifi.avro.AvroReader", "name": "Avro Reader"}
})
nifi_call("PUT", f"controller-services/{avro_reader['id']}/run-status", {"revision": avro_reader["revision"], "state": "ENABLED"})
print(f"Avro Reader: {avro_reader['id'][:12]}")

# JsonRecordSetWriter
json_writer = nifi_call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {"type": "org.apache.nifi.json.JsonRecordSetWriter", "name": "JSON Writer"}
})
nifi_call("PUT", f"controller-services/{json_writer['id']}/run-status", {"revision": json_writer["revision"], "state": "ENABLED"})
print(f"JSON Writer: {json_writer['id'][:12]}")
time.sleep(2)

# === Create processors (proven pattern) ===

SQL = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename,
CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon,
CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

# 1. ExecuteSQL
p1 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQL",
        "name": "1. Extract Apple Photos",
        "position": {"x": 400, "y": 0},
        "config": {
            "properties": {"Database Connection Pooling Service": DBCP, "SQL select query": SQL},
            "autoTerminatedRelationships": ["failure"],
            "schedulingPeriod": "1 day"
        }
    }
})
print(f"1. Extract: {p1['id'][:12]}")

# 2. ConvertRecord (Avro→JSON) — proven pattern from #1652
p2 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ConvertRecord",
        "name": "2. Avro to JSON",
        "position": {"x": 400, "y": 200},
        "config": {
            "properties": {"record-reader": avro_reader["id"], "record-writer": json_writer["id"]},
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
print(f"2. Convert: {p2['id'][:12]}")

# 3. SSH to Library — generate thumbnails + output N-Triples
p3 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "3. Enrich + N-Triples (SSH Library)",
        "position": {"x": 400, "y": 400},
        "config": {
            "properties": {
                "Command Path": "/usr/bin/ssh",
                "Command Arguments": "192.168.86.36;python3;/Users/jeffbridwell/CascadeProjects/architect/scripts/generate-thumbnails-library.py",
                "Argument Delimiter": ";"
            },
            "autoTerminatedRelationships": ["nonzero status", "original"]
        }
    }
})
print(f"3. Enrich: {p3['id'][:12]}")

# 4. InvokeHTTP — PUT to Fuseki (proven pattern: PUT, not POST)
p4 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.InvokeHTTP",
        "name": "4. Load to Fuseki",
        "position": {"x": 400, "y": 600},
        "config": {
            "properties": {
                "HTTP URL": "http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/source/apple-photos",
                "HTTP Method": "PUT",
                "Content-Type": "application/n-triples"
            },
            "autoTerminatedRelationships": ["Response", "Retry", "No Retry", "Failure", "Original"]
        }
    }
})
print(f"4. Fuseki: {p4['id'][:12]}")

# === Connections ===
for src, dst, rels, name in [
    (p1["id"], p2["id"], ["success"], "Extract→Convert"),
    (p2["id"], p3["id"], ["success"], "Convert→Enrich"),
    (p3["id"], p4["id"], ["output stream"], "Enrich→Fuseki"),
]:
    nifi_call("POST", f"process-groups/{PG}/connections", {
        "revision": {"version": 0},
        "component": {
            "source": {"id": src, "groupId": PG, "type": "PROCESSOR"},
            "destination": {"id": dst, "groupId": PG, "type": "PROCESSOR"},
            "selectedRelationships": rels,
            "name": name
        }
    })
    print(f"  {name}")

# Validate
procs = nifi_call("GET", f"process-groups/{PG}/processors")
all_valid = True
for p in procs.get("processors", []):
    vs = p["component"].get("validationErrors", [])
    if vs: all_valid = False
    print(f"  {p['component']['name'][:50]:50} {'VALID' if not vs else 'INVALID: ' + vs[0][:60]}")

if all_valid:
    nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
    print("\nPipeline RUNNING")
else:
    print("\nFix errors first")
