#!/usr/bin/env python3
"""The property name IS 'SQL select query' (lowercase s), not 'SQL Query'."""
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

SQL = """SELECT CAST(a.ZUUID AS TEXT) as uuid, CAST(b.ZORIGINALFILENAME AS TEXT) as filename,
CAST(datetime(a.ZDATECREATED + 978307200, 'unixepoch') AS TEXT) as dateTaken,
CASE WHEN a.ZLATITUDE > -90 AND a.ZLATITUDE < 90 THEN CAST(a.ZLATITUDE AS TEXT) ELSE NULL END as lat,
CASE WHEN a.ZLONGITUDE > -180 AND a.ZLONGITUDE < 180 THEN CAST(a.ZLONGITUDE AS TEXT) ELSE NULL END as lon,
CAST(a.ZWIDTH AS TEXT) as width, CAST(a.ZHEIGHT AS TEXT) as height,
CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"""

procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    if "Extract" in p["component"]["name"]:
        pid = p["id"]
        rev = p["revision"]["version"]
        result = nifi_call("PUT", f"processors/{pid}", {
            "revision": {"version": rev},
            "component": {
                "id": pid,
                "config": {
                    "properties": {
                        "SQL select query": SQL,
                        "Content Output Strategy": "Avro"
                    },
                    "schedulingPeriod": "10 sec"
                }
            }
        })
        vs = result.get("component", {}).get("validationErrors", [])
        print(f"Extract: {'VALID' if not vs else vs}")

        rev = result["revision"]["version"]
        nifi_call("PUT", f"processors/{pid}/run-status", {"revision": {"version": rev}, "state": "RUNNING"})
        print("Started")
        break

time.sleep(15)
status = nifi_call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: In={s['flowFilesIn']} Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
