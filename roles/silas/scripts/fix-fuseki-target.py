#!/usr/bin/env python3
"""Fix Fuseki target: apple-photos source graph, not canonical."""
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
time.sleep(3)

# Find Fuseki processor
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    if "Fuseki" in p["component"]["name"]:
        pid = p["id"]
        rev = p["revision"]["version"]
        result = nifi_call("PUT", f"processors/{pid}", {
            "revision": {"version": rev},
            "component": {
                "id": pid,
                "config": {
                    "properties": {
                        "HTTP URL": "http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/source/apple-photos"
                    }
                }
            }
        })
        print(f"Updated target: urn:gathering:photos/source/apple-photos")
        break

# Clear anything written to wrong graph
subprocess.run(["curl", "-sk", "-X", "DELETE", "http://localhost:3030/pods/data?graph=urn:gathering:photos/canonical"], capture_output=True)
print("Cleared canonical graph")

# Drop all queued flowfiles
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    nifi_call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
time.sleep(2)
print("Dropped queues")

nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("RUNNING — writing to source graph")
