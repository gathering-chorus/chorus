#!/usr/bin/env python3
"""Add ConvertAvroToJSON processor between Apple Extract and Thumbnail SSH."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"
APPLE = "29baf61e-019d-1000-d44c-1ab923817682"
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

# Delete existing Apple→Thumb connection
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    src = conn["component"]["source"]["id"]
    dst = conn["component"]["destination"]["id"]
    if src == APPLE and dst == THUMB:
        crev = conn["revision"]["version"]
        nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
        print(f"Deleted Apple→Thumb connection")
        time.sleep(1)
        break

# Create ConvertAvroToJSON processor
result = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.avro.ConvertAvroToJSON",
        "name": "Convert Avro to JSON",
        "position": {"x": 250, "y": 200},
        "config": {
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
converter_id = result.get("id", "")
if not converter_id:
    # Try alternate class name
    result = nifi_call("POST", f"process-groups/{PG}/processors", {
        "revision": {"version": 0},
        "component": {
            "type": "org.apache.nifi.processors.standard.ConvertAvroToJSON",
            "name": "Convert Avro to JSON",
            "position": {"x": 250, "y": 200},
            "config": {
                "autoTerminatedRelationships": ["failure"]
            }
        }
    })
    converter_id = result.get("id", "")

if converter_id:
    print(f"Created ConvertAvroToJSON: {converter_id[:12]}")
else:
    print(f"Failed to create converter: {json.dumps(result)[:200]}")
    # List available Avro processors
    types = nifi_call("GET", "flow/processor-types")
    avro = [t["type"] for t in types.get("processorTypes", []) if "avro" in t["type"].lower() or "json" in t["type"].lower()]
    print(f"Available Avro/JSON types: {avro[:10]}")
    exit(1)

# Connect Apple → Converter
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": APPLE, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": converter_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "Apple→AvroToJSON"
    }
})
print("Connected Apple→Converter")

# Connect Converter → Thumb
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": converter_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": THUMB, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "AvroToJSON→Thumb"
    }
})
print("Connected Converter→Thumb")

# Start PG
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
print("Pipeline restarted")
