#!/usr/bin/env python3
import json, subprocess, time
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

SQL = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename, CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken, CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat, CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon, CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height, CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Stop
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "STOPPED"})], capture_output=True)
time.sleep(3)

# Get rev
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
d = json.loads(r.stdout)
rev = d["revision"]["version"]

# Set SQL select query
payload = json.dumps({"revision": {"version": rev}, "component": {"id": PROC, "config": {"properties": {"SQL select query": SQL}}}})
r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{PROC}", "-d", payload], capture_output=True, text=True)
d2 = json.loads(r2.stdout)
sql_val = d2["component"]["config"]["properties"].get("SQL select query", "NOT SET")
print(f"SQL set: {sql_val[:60]}...")

# Start
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "RUNNING"})], capture_output=True)
time.sleep(15)

# Check
r3 = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/flow/process-groups/{PG}/status"], capture_output=True, text=True)
s = json.loads(r3.stdout)["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
