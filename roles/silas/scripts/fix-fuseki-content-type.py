#!/usr/bin/env python3
import json, subprocess, time
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Stop PG
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "STOPPED"})], capture_output=True)
time.sleep(3)

# Find Fuseki processor
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/process-groups/{PG}/processors"], capture_output=True, text=True)
d = json.loads(r.stdout)
for p in d.get("processors", []):
    if "Fuseki" in p["component"]["name"]:
        pid = p["id"]
        rev = p["revision"]["version"]

        payload = json.dumps({
            "revision": {"version": rev},
            "component": {
                "id": pid,
                "config": {
                    "properties": {
                        "Request Content-Type": "application/n-triples"
                    }
                }
            }
        })
        with open("/tmp/nifi-payload.json", "w") as f:
            f.write(payload)

        r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{pid}", "-d", f"@/tmp/nifi-payload.json"], capture_output=True, text=True)
        d2 = json.loads(r2.stdout)
        rct = d2["component"]["config"]["properties"].get("Request Content-Type", "?")
        print(f"Request Content-Type: {rct}")
        break

# Start PG
subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/flow/process-groups/{PG}", "-d", json.dumps({"id": PG, "state": "RUNNING"})], capture_output=True)
print("RUNNING")
