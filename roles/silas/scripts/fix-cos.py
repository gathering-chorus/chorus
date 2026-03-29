#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()

# Get current rev
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}"], capture_output=True, text=True)
d = json.loads(r.stdout)
rev = d["revision"]["version"]
cos = d["component"]["config"]["properties"]["Content Output Strategy"]
print(f"Before: COS={cos}, rev={rev}")

# Update
payload = json.dumps({
    "revision": {"version": rev},
    "component": {"id": PROC, "config": {"properties": {"Content Output Strategy": "ORIGINAL"}}}
})
r2 = subprocess.run(["curl", "-sk", "-X", "PUT", "-H", f"Authorization: Bearer {token}", "-H", "Content-Type: application/json", f"{NIFI}/processors/{PROC}", "-d", payload], capture_output=True, text=True)
d2 = json.loads(r2.stdout)
cos2 = d2["component"]["config"]["properties"]["Content Output Strategy"]
rev2 = d2["revision"]["version"]
vs = d2["component"].get("validationErrors", [])
print(f"After: COS={cos2}, rev={rev2}, valid={'yes' if not vs else vs}")
