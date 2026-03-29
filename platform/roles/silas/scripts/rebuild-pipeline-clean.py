#!/usr/bin/env python3
"""
Clean rebuild of NiFi Photos Pipeline with correct NiFi 2.8 processors.
No Avro — JSON throughout.
ExecuteSQLRecord → SplitRecord → ExecuteStreamCommand(SSH) → InvokeHTTP(Fuseki)
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
    except: return {"_raw": r.stdout[:200] if r.stdout else "empty"}

# === STOP AND CLEAN ===
print("Stopping pipeline...")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

print("Dropping queues and deleting connections...")
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    cid = conn["id"]
    crev = conn["revision"]["version"]
    nifi_call("POST", f"flowfile-queues/{cid}/drop-requests")
    time.sleep(1)
    result = nifi_call("DELETE", f"connections/{cid}?version={crev}")
    time.sleep(0.5)
print(f"  Deleted {len(conns.get('connections', []))} connections")

print("Deleting processors...")
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    pid = p["id"]
    prev = p["revision"]["version"]
    nifi_call("DELETE", f"processors/{pid}?version={prev}")
    time.sleep(0.5)
print(f"  Deleted {len(procs.get('processors', []))} processors")

# Delete old controller services except DBCP
print("Cleaning controller services...")
css = nifi_call("GET", f"flow/process-groups/{PG}/controller-services")
for cs in css.get("controllerServices", []):
    csid = cs["id"]
    if csid == DBCP:
        continue
    csrev = cs["revision"]["version"]
    state = cs["component"]["state"]
    if state == "ENABLED":
        nifi_call("PUT", f"controller-services/{csid}/run-status", {"revision": {"version": csrev}, "state": "DISABLED"})
        time.sleep(2)
        cs = nifi_call("GET", f"controller-services/{csid}")
        csrev = cs["revision"]["version"]
    nifi_call("DELETE", f"controller-services/{csid}?version={csrev}")
    print(f"  Deleted service {cs.get('component',{}).get('name','?')}")

# === CREATE CONTROLLER SERVICES ===
print("\nCreating controller services...")

# JsonRecordSetWriter
writer = nifi_call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.json.JsonRecordSetWriter",
        "name": "JSON Writer"
    }
})
writer_id = writer["id"]
print(f"  JSON Writer: {writer_id[:12]}")

# JsonTreeReader
reader = nifi_call("POST", f"process-groups/{PG}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.json.JsonTreeReader",
        "name": "JSON Reader"
    }
})
reader_id = reader["id"]
print(f"  JSON Reader: {reader_id[:12]}")

# Enable both
time.sleep(1)
for sid, name in [(writer_id, "Writer"), (reader_id, "Reader")]:
    cs = nifi_call("GET", f"controller-services/{sid}")
    csrev = cs["revision"]["version"]
    nifi_call("PUT", f"controller-services/{sid}/run-status", {"revision": {"version": csrev}, "state": "ENABLED"})
    print(f"  Enabled {name}")
time.sleep(2)

# === CREATE PROCESSORS ===
print("\nCreating processors...")

# 1. ExecuteSQLRecord — JSON output directly
sql = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename,
CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon,
CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

p1 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQLRecord",
        "name": "1. Extract Apple Photos (JSON)",
        "position": {"x": 400, "y": 100},
        "config": {
            "properties": {
                "Database Connection Pooling Service": DBCP,
                "SQL Query": sql,
                "Record Writer": writer_id
            },
            "autoTerminatedRelationships": ["failure"],
            "schedulingPeriod": "1 day"
        }
    }
})
p1_id = p1["id"]
print(f"  Extract: {p1_id[:12]}")

# 2. SplitRecord — one JSON record per flowfile
p2 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.SplitRecord",
        "name": "2. Split to Individual Records",
        "position": {"x": 400, "y": 250},
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
p2_id = p2["id"]
print(f"  Split: {p2_id[:12]}")

# 3. ExecuteStreamCommand — SSH to Library for thumbnail generation
p3 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "3. Generate Thumbnail (SSH to Library)",
        "position": {"x": 400, "y": 400},
        "config": {
            "properties": {
                "Command Path": "/usr/bin/ssh",
                "Command Arguments": "192.168.86.36;python3;/Users/jeffbridwell/CascadeProjects/architect/scripts/generate-thumbnails-library.py",
                "Argument Delimiter": ";",
                "Ignore STDIN": "false"
            },
            "autoTerminatedRelationships": ["nonzero status", "original"]
        }
    }
})
p3_id = p3["id"]
print(f"  Thumbnail: {p3_id[:12]}")

# 4. InvokeHTTP — POST to Fuseki
p4 = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.InvokeHTTP",
        "name": "4. Write to Fuseki",
        "position": {"x": 400, "y": 550},
        "config": {
            "properties": {
                "HTTP URL": "http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/canonical",
                "HTTP Method": "POST",
                "Content-Type": "application/n-triples"
            },
            "autoTerminatedRelationships": ["Response", "Retry", "No Retry", "Failure", "Original"]
        }
    }
})
p4_id = p4["id"]
print(f"  Fuseki: {p4_id[:12]}")

# === CONNECTIONS ===
print("\nCreating connections...")
for src, dst, rels, name in [
    (p1_id, p2_id, ["success"], "Extract→Split"),
    (p2_id, p3_id, ["splits"], "Split→Thumbnail"),
    (p3_id, p4_id, ["output stream"], "Thumbnail→Fuseki"),
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

# === VALIDATE ===
print("\nValidating...")
procs = nifi_call("GET", f"process-groups/{PG}/processors")
all_valid = True
for p in procs.get("processors", []):
    c = p["component"]
    vs = c.get("validationErrors", [])
    status = "VALID" if not vs else f"INVALID: {vs[0][:80]}"
    if vs: all_valid = False
    print(f"  {c['name'][:50]:50} {status}")

if all_valid:
    print("\n=== All processors valid. Starting pipeline ===")
    nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
    print("Pipeline RUNNING")
else:
    print("\n=== Some processors invalid. NOT starting. Fix errors first. ===")
