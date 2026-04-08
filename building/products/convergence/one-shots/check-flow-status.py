#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

token_r = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True)
token = token_r.stdout.strip()

r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/flow/process-groups/{PG}/status"], capture_output=True, text=True)
d = json.loads(r.stdout)
s = d["processGroupStatus"]["aggregateSnapshot"]
print(f"In={s['flowFilesIn']} Queued={s['flowFilesQueued']} Out={s['flowFilesOut']} Read={s['bytesRead']} Written={s['bytesWritten']} Threads={s['activeThreadCount']}")
