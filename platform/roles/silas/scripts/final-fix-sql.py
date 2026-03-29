#!/usr/bin/env python3
"""Set SQL Query (the REAL property), remove dynamic SQL select query."""
import json, subprocess, time
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
PROC = "2a2d9c71-019d-1000-6b67-f064d89fde5a"

SQL = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename, CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken, CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat, CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon, CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height, CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Stop PG
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "STOPPED"})], capture_output=True)
time.sleep(3)

# Get rev
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
d = json.loads(r.stdout)
rev = d["revision"]["version"]

# Set SQL Query (real property), null out dynamic SQL select query
payload = json.dumps({
    "revision": {"version": rev},
    "component": {
        "id": PROC,
        "config": {
            "properties": {
                "SQL Query": SQL,
                "SQL select query": None
            }
        }
    }
})
r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{PROC}", "-d", payload], capture_output=True, text=True)
d2 = json.loads(r2.stdout)
sql_real = d2["component"]["config"]["properties"].get("SQL Query", "NOT SET")
sql_dyn = d2["component"]["config"]["properties"].get("SQL select query", "REMOVED")
vs = d2["component"].get("validationErrors", [])
print(f"SQL Query: {sql_real[:60]}...")
print(f"SQL select query (dynamic): {sql_dyn}")
print(f"Valid: {'yes' if not vs else vs}")

# Start
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "RUNNING"})], capture_output=True)
time.sleep(15)

# Check flow
r3 = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/flow/process-groups/{PG}/status"], capture_output=True, text=True)
s = json.loads(r3.stdout)["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
