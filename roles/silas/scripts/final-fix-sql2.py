#!/usr/bin/env python3
"""Write payload to file to avoid shell escaping issues."""
import json, subprocess, time, tempfile, os
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
PROC = "2a2d9c71-019d-1000-6b67-f064d89fde5a"

SQL = "SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename, CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken, CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat, CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon, CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height, CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Get rev
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
d = json.loads(r.stdout)
rev = d["revision"]["version"]
print(f"Rev: {rev}")

# Write payload to temp file
payload = {
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
}
payload_file = "/tmp/nifi-payload.json"
with open(payload_file, "w") as f:
    json.dump(payload, f)

# PUT using file
r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{PROC}", "-d", f"@{payload_file}"], capture_output=True, text=True)
print(f"Response length: {len(r2.stdout)}")
if r2.stdout.strip():
    d2 = json.loads(r2.stdout)
    sql_val = d2.get("component", {}).get("config", {}).get("properties", {}).get("SQL Query", "NOT SET")
    vs = d2.get("component", {}).get("validationErrors", [])
    print(f"SQL Query: {sql_val[:60]}...")
    print(f"Valid: {'yes' if not vs else vs}")
else:
    print(f"Empty response. Status may have changed.")
    # Check current state
    r3 = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
    d3 = json.loads(r3.stdout)
    print(f"SQL Query now: {d3['component']['config']['properties'].get('SQL Query', 'NOT SET')[:60]}")

os.unlink(payload_file)
