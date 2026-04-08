#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token_r = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True)
token = token_r.stdout.strip()

# Get all processors in PG
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/process-groups/{PG}/processors"], capture_output=True, text=True)
d = json.loads(r.stdout)
for p in d.get("processors", []):
    c = p["component"]
    name = c["name"]
    state = c["state"]
    ptype = c["type"].split(".")[-1]
    vs = c.get("validationErrors", [])
    bs = p.get("bulletins", [])
    status = ""
    if vs: status = f" INVALID: {vs[0][:80]}"
    if bs: status += f" BULLETIN: {bs[0].get('bulletin',{}).get('message','')[:80]}"
    print(f"{state:8} {ptype:30} {name}{status}")
