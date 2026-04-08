#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/process-groups/{PG}/processors"], capture_output=True, text=True)
d = json.loads(r.stdout)
for p in d.get("processors", []):
    if "Fuseki" in p["component"]["name"]:
        props = p["component"]["config"]["properties"]
        print(f"HTTP URL: {props.get('HTTP URL', 'NOT SET')}")
        print(f"HTTP Method: {props.get('HTTP Method', 'NOT SET')}")
        print(f"Content-Type: {props.get('Content-Type', 'NOT SET')}")
        print(f"All props with values:")
        for k, v in props.items():
            if v: print(f"  {k}: {v}")
