#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token_r = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True)
token = token_r.stdout.strip()

r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/process-groups/{PG}/connections"], capture_output=True, text=True)
d = json.loads(r.stdout)
for conn in d.get("connections", []):
    c = conn["component"]
    q = conn["status"]["aggregateSnapshot"]
    name = c.get("name", "unnamed")
    queued = q.get("queuedCount", "0")
    queued_bytes = q.get("queuedSize", "0")
    in_count = q.get("flowFilesIn", 0)
    out_count = q.get("flowFilesOut", 0)
    print(f"{name:30} queued={queued:>8} in={in_count:>8} out={out_count:>8}")
