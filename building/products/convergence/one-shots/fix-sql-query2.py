#!/usr/bin/env python3
"""Fix SQL query — handle sentinel lat/lon values."""
import json, subprocess

NIFI = "https://192.168.86.242:8443/nifi-api"
PROC = "29baf61e-019d-1000-d44c-1ab923817682"

def nifi_call(method, path, data=None):
    token_r = subprocess.run(
        ["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"],
        capture_output=True, text=True
    )
    token = token_r.stdout.strip()
    args = ["curl", "-sk", "-X", method,
            "-H", f"Authorization: Bearer {token}",
            "-H", "Content-Type: application/json",
            f"{NIFI}/{path}"]
    if data:
        args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    return json.loads(r.stdout) if r.stdout.strip() else {}

proc = nifi_call("GET", f"processors/{PROC}")
rev = proc["revision"]["version"]
state = proc["component"]["state"]

if state == "RUNNING":
    nifi_call("PUT", f"processors/{PROC}/run-status", {"revision": {"version": rev}, "state": "STOPPED"})
    import time; time.sleep(2)
    proc = nifi_call("GET", f"processors/{PROC}")
    rev = proc["revision"]["version"]

# Filter sentinel lat/lon values — Apple uses -180 as "no location"
sql = """SELECT a.ZUUID as uuid, b.ZORIGINALFILENAME as filename,
datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN a.ZLATITUDE ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN a.ZLONGITUDE ELSE NULL END as lon,
a.ZWIDTH as width, a.ZHEIGHT as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a
JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

result = nifi_call("PUT", f"processors/{PROC}", {
    "revision": {"version": rev},
    "component": {"id": PROC, "config": {"properties": {"SQL Query": sql}}}
})
print(f"SQL updated, rev {result['revision']['version']}")

rev = result["revision"]["version"]
nifi_call("PUT", f"processors/{PROC}/run-status", {"revision": {"version": rev}, "state": "RUNNING"})
print("Processor restarted")
