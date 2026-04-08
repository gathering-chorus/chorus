#!/usr/bin/env python3
import json, subprocess
NIFI = "https://192.168.86.242:8443/nifi-api"
PROC = "2a22d6ee-019d-1000-48d4-bb4bf7d38f77"

token = subprocess.run(["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"], capture_output=True, text=True).stdout.strip()
r = subprocess.run(["curl", "-sk", "-H", f"Authorization: Bearer {token}", f"{NIFI}/processors/{PROC}/descriptors?propertyName=Content+Output+Strategy"], capture_output=True, text=True)
d = json.loads(r.stdout)
desc = d.get("propertyDescriptor", {})
for v in desc.get("allowableValues", []):
    av = v.get("allowableValue", {})
    print(f"  value='{av.get('value')}' display='{av.get('displayName')}'")
