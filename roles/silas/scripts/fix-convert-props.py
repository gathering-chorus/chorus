#!/usr/bin/env python3
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

# Find the ConvertRecord processor
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    if "Avro to JSON" in p["component"]["name"]:
        pid = p["id"]
        props = p["component"]["config"]["properties"]
        print(f"Current props: {json.dumps(props, indent=2)}")

        # Fix: use correct property names
        rev = p["revision"]["version"]

        # Find service IDs
        css = nifi_call("GET", f"flow/process-groups/{PG}/controller-services")
        avro_id = json_id = None
        for cs in css.get("controllerServices", []):
            if cs["component"]["name"] == "Avro Reader": avro_id = cs["id"]
            if cs["component"]["name"] == "JSON Writer": json_id = cs["id"]

        print(f"Avro Reader ID: {avro_id}")
        print(f"JSON Writer ID: {json_id}")

        result = nifi_call("PUT", f"processors/{pid}", {
            "revision": {"version": rev},
            "component": {
                "id": pid,
                "config": {
                    "properties": {
                        "Record Reader": avro_id,
                        "Record Writer": json_id
                    }
                }
            }
        })
        vs = result.get("component", {}).get("validationErrors", [])
        print(f"After fix: {'VALID' if not vs else vs}")
        break

# Start
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("RUNNING")
