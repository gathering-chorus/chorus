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

# Find extract processor
procs = nifi_call("GET", f"process-groups/{PG}/processors")
for p in procs.get("processors", []):
    if "Extract" in p["component"]["name"]:
        pid = p["id"]
        rev = p["revision"]["version"]
        props = p["component"]["config"]["properties"]

        # Get the property descriptor to see allowed values
        token_r = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True)
        token = token_r.stdout.strip()

        # Check all properties
        print(f"Current properties:")
        for k, v in props.items():
            print(f"  {k}: {v}")

        # The issue: NiFi 2.8 ExecuteSQL default "Content Output Strategy" = "EMPTY" means
        # flowfile body is empty, data goes to attributes. We need it to write Avro.
        # Try setting it to the correct value
        for val in ["Avro", "avro", "AVRO", "Use Avro", "USE_AVRO_CONTENT_STRATEGY"]:
            result = nifi_call("PUT", f"processors/{pid}", {
                "revision": {"version": rev},
                "component": {
                    "id": pid,
                    "config": {
                        "properties": {"Content Output Strategy": val}
                    }
                }
            })
            actual = result.get("component", {}).get("config", {}).get("properties", {}).get("Content Output Strategy")
            if actual and actual != "EMPTY":
                print(f"  Set Content Output Strategy = {actual} (tried {val})")
                rev = result["revision"]["version"]
                break
            rev = result.get("revision", {}).get("version", rev)

        # Try starting
        nifi_call("PUT", f"processors/{pid}/run-status", {"revision": {"version": rev}, "state": "RUNNING"})
        time.sleep(3)
        p2 = nifi_call("GET", f"processors/{pid}")
        print(f"\nState: {p2['component']['state']}")
        bs = p2.get("bulletins", [])
        for b in bs[:2]:
            print(f"Bulletin: {b.get('bulletin',{}).get('message','')[:150]}")
        break
