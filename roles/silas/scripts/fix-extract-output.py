#!/usr/bin/env python3
"""Fix: Content Output Strategy must be 'AVRO', not 'EMPTY'."""
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

nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(2)

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
                        "Content Output Strategy": "USE_AVRO",
                        "SQL select query": None  # Remove stale dynamic property
                    },
                    "schedulingPeriod": "10 sec"
                }
            }
        })
        props = result.get("component", {}).get("config", {}).get("properties", {})
        print(f"Content Output Strategy: {props.get('Content Output Strategy')}")
        vs = result.get("component", {}).get("validationErrors", [])
        print(f"Validation: {'VALID' if not vs else vs}")
        break

nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})

time.sleep(15)
status = nifi_call("GET", f"flow/process-groups/{PG}/status")
s = status["processGroupStatus"]["aggregateSnapshot"]
print(f"Flow: In={s['flowFilesIn']} Queued={s['flowFilesQueued']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
