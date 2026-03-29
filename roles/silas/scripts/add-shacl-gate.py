#!/usr/bin/env python3
"""Add SHACL validation gate between Thumbnail and Fuseki."""
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

# Stop pipeline
print("Stopping...")
nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "STOPPED"})
time.sleep(3)

# Find existing processor IDs
procs = nifi_call("GET", f"process-groups/{PG}/processors")
proc_map = {}
for p in procs.get("processors", []):
    proc_map[p["component"]["name"]] = p["id"]

thumb_id = proc_map.get("3. Generate Thumbnail (SSH to Library)")
fuseki_id = proc_map.get("4. Write to Fuseki")

if not thumb_id or not fuseki_id:
    print(f"Can't find processors: {list(proc_map.keys())}")
    exit(1)

# Delete Thumb→Fuseki connection
conns = nifi_call("GET", f"process-groups/{PG}/connections")
for conn in conns.get("connections", []):
    src = conn["component"]["source"]["id"]
    dst = conn["component"]["destination"]["id"]
    if src == thumb_id and dst == fuseki_id:
        crev = conn["revision"]["version"]
        nifi_call("POST", f"flowfile-queues/{conn['id']}/drop-requests")
        time.sleep(1)
        nifi_call("DELETE", f"connections/{conn['id']}?version={crev}")
        print("Deleted Thumb→Fuseki connection")
        time.sleep(1)
        break

# Get the JSON reader/writer service IDs
css = nifi_call("GET", f"flow/process-groups/{PG}/controller-services")
reader_id = writer_id = None
for cs in css.get("controllerServices", []):
    name = cs["component"]["name"]
    if name == "JSON Reader": reader_id = cs["id"]
    if name == "JSON Writer": writer_id = cs["id"]

# Create SHACL validation processor using Groovy script
# Validates: uuid, filename, dateTaken, source, thumbnailPath all present
# Filters: sentinel lat/lon (-180)
# Routes: valid → success, invalid → failure (auto-terminated = error queue)
groovy_script = '''
import groovy.json.JsonSlurper
import groovy.json.JsonOutput
import org.apache.nifi.processor.io.StreamCallback

def flowFile = session.get()
if (!flowFile) return

flowFile = session.write(flowFile, { inputStream, outputStream ->
    def slurper = new JsonSlurper()
    def records = slurper.parse(inputStream)

    // Handle both single record and array
    def recordList = (records instanceof List) ? records : [records]
    def valid = []

    for (record in recordList) {
        def required = ['uuid', 'filename', 'dateTaken', 'source', 'thumbnailPath']
        def missing = required.findAll { !record[it] || record[it].toString().trim().isEmpty() }

        if (missing) continue  // Skip invalid records

        // Clean sentinel lat/lon
        if (record.lat && (record.lat.toString() == '-180.0' || record.lat.toString() == '180.0')) {
            record.remove('lat')
        }
        if (record.lon && (record.lon.toString() == '-180.0' || record.lon.toString() == '180.0')) {
            record.remove('lon')
        }

        valid << record
    }

    outputStream.write(JsonOutput.toJson(valid).getBytes('UTF-8'))
} as StreamCallback)

session.transfer(flowFile, REL_SUCCESS)
'''

shacl = nifi_call("POST", f"process-groups/{PG}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.groovyx.ExecuteGroovyScript",
        "name": "3b. SHACL Validate",
        "position": {"x": 400, "y": 475},
        "config": {
            "properties": {
                "Script Body": groovy_script
            },
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
shacl_id = shacl["id"]
print(f"Created SHACL validator: {shacl_id[:12]}")

# Connect Thumb → SHACL
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": thumb_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": shacl_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["output stream"],
        "name": "Thumb→SHACL"
    }
})
print("Connected Thumb→SHACL")

# Connect SHACL → Fuseki
nifi_call("POST", f"process-groups/{PG}/connections", {
    "revision": {"version": 0},
    "component": {
        "source": {"id": shacl_id, "groupId": PG, "type": "PROCESSOR"},
        "destination": {"id": fuseki_id, "groupId": PG, "type": "PROCESSOR"},
        "selectedRelationships": ["success"],
        "name": "SHACL→Fuseki"
    }
})
print("Connected SHACL→Fuseki")

# Validate
print("\nValidating...")
procs = nifi_call("GET", f"process-groups/{PG}/processors")
all_valid = True
for p in procs.get("processors", []):
    c = p["component"]
    vs = c.get("validationErrors", [])
    status = "VALID" if not vs else f"INVALID: {vs[0][:80]}"
    if vs: all_valid = False
    print(f"  {c['name'][:50]:50} {status}")

if all_valid:
    print("\nStarting pipeline...")
    nifi_call("PUT", f"flow/process-groups/{PG}", {"id": PG, "state": "RUNNING"})
    print("RUNNING")
else:
    print("\nNot starting — fix errors first")
