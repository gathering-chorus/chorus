#!/usr/bin/env python3
"""Fix Thumbnail SSH processor now that SSH from Bedroom→Library works."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
THUMB = "2a0307ff-019d-1000-a5e9-52eb137b506b"

def nifi_call(method, path, data=None):
    token_r = subprocess.run(
        ["curl", "-sk", f"{NIFI}/access/token", "-d", "username=admin&password=nifi-gathering-2026"],
        capture_output=True, text=True)
    token = token_r.stdout.strip()
    args = ["curl", "-sk", "-X", method, "-H", f"Authorization: Bearer {token}",
            "-H", "Content-Type: application/json", f"{NIFI}/{path}"]
    if data: args.extend(["-d", json.dumps(data)])
    r = subprocess.run(args, capture_output=True, text=True)
    try: return json.loads(r.stdout) if r.stdout.strip() else {}
    except: return {}

# Stop PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Update processor config
proc = nifi_call("GET", f"processors/{THUMB}")
rev = proc["revision"]["version"]

result = nifi_call("PUT", f"processors/{THUMB}", {
    "revision": {"version": rev},
    "component": {
        "id": THUMB,
        "config": {
            "properties": {
                "Command Path": "/usr/bin/ssh",
                "Command Arguments": "192.168.86.36;python3;/Users/jeffbridwell/CascadeProjects/architect/scripts/generate-thumbnails-library.py",
                "Argument Delimiter": ";",
                "Ignore STDIN": "false"
            },
            "autoTerminatedRelationships": ["nonzero status", "original"],
            "schedulingPeriod": "0 sec"
        }
    }
})
vs = result.get("component", {}).get("validationErrors", [])
print(f"Configured: errors={vs[:1] if vs else 'none'}")

# Start PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("Pipeline restarted")
