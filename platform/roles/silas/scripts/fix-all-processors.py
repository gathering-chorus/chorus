#!/usr/bin/env python3
"""Fix all processor configs in the photos pipeline PG."""
import json, subprocess, time

NIFI = "https://192.168.86.242:8443/nifi-api"
PG = "29ba37cc-019d-1000-3647-41116b669ef2"

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

def get_rev(proc_id):
    d = nifi_call("GET", f"processors/{proc_id}")
    return d["revision"]["version"]

def stop_pg():
    nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
    time.sleep(3)

def start_pg():
    nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})

# Stop everything first
print("Stopping pipeline...")
stop_pg()

# Get available script engines
r = nifi_call("GET", f"process-groups/{PG}/processors")
processors = {p["component"]["name"]: p for p in r.get("processors", [])}

# Find the correct script engine name
# NiFi 2.x uses "Groovy" not "python" for ExecuteScript
# Check what's available
print("Available processors:")
for name, p in processors.items():
    pid = p["id"]
    ptype = p["component"]["type"].split(".")[-1]
    state = p["component"]["state"]
    print(f"  {pid[:12]} | {ptype:30} | {state:8} | {name}")

# 1. Fix Thumbnail SSH processor — connect 'original' relationship
THUMB_SSH = None
for name, p in processors.items():
    if "SSH" in name:
        THUMB_SSH = p["id"]
        break

if THUMB_SSH:
    # Auto-terminate the 'original' relationship
    rev = get_rev(THUMB_SSH)
    result = nifi_call("PUT", f"processors/{THUMB_SSH}", {
        "revision": {"version": rev},
        "component": {
            "id": THUMB_SSH,
            "config": {
                "autoTerminatedRelationships": ["nonzero status", "original"]
            }
        }
    })
    print(f"Fixed Thumbnail SSH — auto-terminated 'original' relationship")

# 2. Delete old Thumbnail ExecuteScript (the one that failed)
OLD_THUMB = None
for name, p in processors.items():
    if name == "1d. Enrich — Thumbnail Generation":
        OLD_THUMB = p["id"]
        break

if OLD_THUMB:
    # Check if it has connections
    conns = nifi_call("GET", f"process-groups/{PG}/connections")
    for conn in conns.get("connections", []):
        if conn["component"]["source"]["id"] == OLD_THUMB or conn["component"]["destination"]["id"] == OLD_THUMB:
            crev = conn["revision"]["version"]
            nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
            time.sleep(0.5)
    rev = get_rev(OLD_THUMB)
    nifi_call("DELETE", f"processors/{OLD_THUMB}?version={rev}")
    print(f"Deleted old thumbnail processor")

# 3. Fix SHACL — change script engine to Groovy
SHACL = None
for name, p in processors.items():
    if "SHACL" in name:
        SHACL = p["id"]
        break

if SHACL:
    rev = get_rev(SHACL)
    # Use Groovy instead of python — NiFi 2.8 may not have python engine
    groovy_script = """
import groovy.json.JsonSlurper
import groovy.json.JsonOutput
import org.apache.nifi.processor.io.StreamCallback

def flowFile = session.get()
if (!flowFile) return

flowFile = session.write(flowFile, { inputStream, outputStream ->
    def slurper = new JsonSlurper()
    def record = slurper.parse(inputStream)
    def required = ['uuid', 'filename', 'dateTaken', 'source', 'thumbnailPath']
    def missing = required.findAll { !record[it] }
    if (missing) record['_shacl_fail'] = missing
    outputStream.write(JsonOutput.toJson(record).getBytes('UTF-8'))
} as StreamCallback)

session.transfer(flowFile, REL_SUCCESS)
"""
    result = nifi_call("PUT", f"processors/{SHACL}", {
        "revision": {"version": rev},
        "component": {
            "id": SHACL,
            "config": {
                "properties": {
                    "Script Engine": "Groovy",
                    "Script Body": groovy_script
                },
                "autoTerminatedRelationships": ["failure"]
            }
        }
    })
    vs = result.get("component", {}).get("validationErrors", [])
    print(f"Fixed SHACL — engine=Groovy, errors={vs[:1] if vs else 'none'}")

# 4. Fix InvokeHTTP — correct property name
FUSEKI = None
for name, p in processors.items():
    if "Fuseki" in name:
        FUSEKI = p["id"]
        break

if FUSEKI:
    rev = get_rev(FUSEKI)
    result = nifi_call("PUT", f"processors/{FUSEKI}", {
        "revision": {"version": rev},
        "component": {
            "id": FUSEKI,
            "config": {
                "properties": {
                    "HTTP URL": "http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/canonical",
                    "HTTP Method": "POST",
                    "Content-Type": "application/n-triples"
                },
                "autoTerminatedRelationships": ["Response", "Retry", "No Retry", "Failure", "Original"]
            }
        }
    })
    vs = result.get("component", {}).get("validationErrors", [])
    print(f"Fixed Fuseki InvokeHTTP — errors={vs[:1] if vs else 'none'}")

# 5. Check all processor states
print("\nFinal status:")
r = nifi_call("GET", f"process-groups/{PG}/processors")
for p in r.get("processors", []):
    c = p["component"]
    vs = c.get("validationErrors", [])
    status = "VALID" if not vs else f"INVALID: {vs[0][:60]}"
    print(f"  {c['state']:8} {c['name'][:50]:50} {status}")

# Start pipeline
print("\nStarting pipeline...")
start_pg()
print("Done")
