#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
OLD_PG = "24861535-019d-1000-aa26-4be6c373d44e"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()
print(f"Token length: {len(token)}")
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/process-groups/{OLD_PG}/processors"], capture_output=True, text=True)
print(f"Response length: {len(r.stdout)}")
d = json.loads(r.stdout)
for p in d.get("processors", []):
    ptype = p["component"]["type"]
    if "SQL" in ptype:
        props = p["component"]["config"]["properties"]
        print(f"Type: {ptype}")
        for k, v in props.items():
            if v and v != "false" and v != "0" and v != "NONE":
                val = str(v)[:80]
                print(f"  {k}: {val}")
