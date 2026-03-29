#!/bin/bash
# Configure NiFi Photos Pipeline — wire all processors and connections
# Card: #1705 | Runs on Bedroom
set -e

NIFI="https://192.168.86.242:8443/nifi-api"
TOKEN=$(curl -sk "$NIFI/access/token" -d "username=admin&password=nifi-gathering-2026" 2>/dev/null)
PG="29ba37cc-019d-1000-3647-41116b669ef2"

# Processor IDs
APPLE="29baf61e-019d-1000-d44c-1ab923817682"
IPHONE="29baf63e-019d-1000-57a7-33734e4630c3"
TAKEOUT="29baf65b-019d-1000-1035-ffccbeda7303"
THUMB="29baf702-019d-1000-3c77-99381f3496ea"
SHACL="29baf722-019d-1000-03a6-d90719b1f108"
FUSEKI="29baf741-019d-1000-ad84-d921edd5609e"
DBCP="29efe61f-019d-1000-6861-7246d90a55fb"

auth() { echo "Authorization: Bearer $TOKEN"; }

get_rev() {
    local id=$1 type=$2
    curl -sk -H "$(auth)" "$NIFI/${type}/${id}" | python3 -c "import json,sys; print(json.load(sys.stdin)['revision']['version'])" 2>/dev/null
}

# Helper: create connection between two processors
connect() {
    local src=$1 dst=$2 rels=$3 name=$4
    python3 -c "
import json, subprocess
payload = {
    'revision': {'version': 0},
    'component': {
        'source': {'id': '$src', 'groupId': '$PG', 'type': 'PROCESSOR'},
        'destination': {'id': '$dst', 'groupId': '$PG', 'type': 'PROCESSOR'},
        'selectedRelationships': $rels,
        'name': '$name'
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'POST',
    '-H', '$(auth)',
    '-H', 'Content-Type: application/json',
    '$NIFI/process-groups/$PG/connections',
    '-d', json.dumps(payload)], capture_output=True, text=True)
d = json.loads(r.stdout)
print(f'  Connected: {d[\"id\"][:12]} — $name')
"
}

echo "=== Configuring Thumbnail Enrichment (ExecuteScript) ==="
REV=$(get_rev $THUMB processors)
python3 -c "
import json, subprocess
script = '''
import json, os, subprocess, sys
from org.apache.nifi.processor.io import StreamCallback
from java.io import BufferedReader, InputStreamReader, BufferedWriter, OutputStreamWriter

class ThumbnailEnrich(StreamCallback):
    def process(self, inputStream, outputStream):
        reader = BufferedReader(InputStreamReader(inputStream, 'UTF-8'))
        content = reader.readLine()
        reader.close()
        record = json.loads(content)

        uuid = record.get('uuid', '')
        bucket = record.get('dateTaken', '')[:7]
        thumb_base = '/Volumes/VideosNew/Gathering/Photos/generated/thumbnails'
        deriv_base = os.path.expanduser('~/Pictures/Photos Library.photoslibrary/resources/derivatives')
        orig_base = os.path.expanduser('~/Pictures/Photos Library.photoslibrary/originals')

        out_dir = os.path.join(thumb_base, bucket)
        out_path = os.path.join(out_dir, uuid.lower() + '.jpg')

        if os.path.exists(out_path) and os.path.getsize(out_path) > 0:
            record['thumbnailPath'] = '/thumbnails/photos/' + bucket + '/' + uuid.lower() + '.jpg'
        else:
            upper = uuid.upper()
            first = upper[0]
            source = None
            for pat in [upper + '_4_5005_c.jpeg', upper + '_1_105_c.jpeg']:
                p = os.path.join(deriv_base, first, pat)
                if os.path.exists(p):
                    source = p
                    break
            if not source:
                for ext in ['.heic', '.HEIC', '.jpeg', '.jpg', '.png']:
                    p = os.path.join(orig_base, first, upper + ext)
                    if os.path.exists(p):
                        source = p
                        break
            if source:
                os.makedirs(out_dir, exist_ok=True)
                os.system('sips -s format jpeg -z 400 300 \"' + source + '\" --out \"' + out_path + '\" >/dev/null 2>&1')
                if os.path.exists(out_path):
                    record['thumbnailPath'] = '/thumbnails/photos/' + bucket + '/' + uuid.lower() + '.jpg'

        writer = BufferedWriter(OutputStreamWriter(outputStream, 'UTF-8'))
        writer.write(json.dumps(record))
        writer.flush()
        writer.close()

flowFile = session.get()
if flowFile:
    if 'thumbnailPath' not in str(flowFile.getAttribute('thumbnailPath') or ''):
        flowFile = session.write(flowFile, ThumbnailEnrich())
    session.transfer(flowFile, REL_SUCCESS)
'''
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$THUMB',
        'config': {
            'properties': {
                'Script Engine': 'python',
                'Script Body': script
            },
            'autoTerminatedRelationships': ['failure']
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', '$(auth)',
    '-H', 'Content-Type: application/json',
    '$NIFI/processors/$THUMB',
    '-d', json.dumps(payload)], capture_output=True, text=True)
try:
    d = json.loads(r.stdout)
    print(f'  Configured: {d[\"component\"][\"name\"]}')
except:
    print(f'  Error: {r.stdout[:200]}')
"

echo "=== Configuring SHACL Validate (ExecuteScript) ==="
REV=$(get_rev $SHACL processors)
python3 -c "
import json, subprocess
script = '''
import json
from org.apache.nifi.processor.io import StreamCallback
from java.io import BufferedReader, InputStreamReader, BufferedWriter, OutputStreamWriter

class ShaclValidate(StreamCallback):
    def process(self, inputStream, outputStream):
        reader = BufferedReader(InputStreamReader(inputStream, 'UTF-8'))
        content = reader.readLine()
        reader.close()
        record = json.loads(content)
        required = ['uuid', 'filename', 'dateTaken', 'source', 'thumbnailPath']
        missing = [f for f in required if not record.get(f)]
        if missing:
            record['_shacl_fail'] = missing
        writer = BufferedWriter(OutputStreamWriter(outputStream, 'UTF-8'))
        writer.write(json.dumps(record))
        writer.flush()
        writer.close()

flowFile = session.get()
if flowFile:
    flowFile = session.write(flowFile, ShaclValidate())
    session.transfer(flowFile, REL_SUCCESS)
'''
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$SHACL',
        'config': {
            'properties': {
                'Script Engine': 'python',
                'Script Body': script
            },
            'autoTerminatedRelationships': ['failure']
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', '$(auth)',
    '-H', 'Content-Type: application/json',
    '$NIFI/processors/$SHACL',
    '-d', json.dumps(payload)], capture_output=True, text=True)
try:
    d = json.loads(r.stdout)
    print(f'  Configured: {d[\"component\"][\"name\"]}')
except:
    print(f'  Error: {r.stdout[:200]}')
"

echo "=== Configuring Fuseki Write (InvokeHTTP) ==="
REV=$(get_rev $FUSEKI processors)
python3 -c "
import json, subprocess
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$FUSEKI',
        'config': {
            'properties': {
                'HTTP Method': 'POST',
                'Remote URL': 'http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/canonical',
                'Content-Type': 'application/n-triples'
            },
            'autoTerminatedRelationships': ['Response', 'Retry', 'No Retry', 'Failure', 'Original']
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', '$(auth)',
    '-H', 'Content-Type: application/json',
    '$NIFI/processors/$FUSEKI',
    '-d', json.dumps(payload)], capture_output=True, text=True)
try:
    d = json.loads(r.stdout)
    print(f'  Configured: {d[\"component\"][\"name\"]}')
except:
    print(f'  Error: {r.stdout[:200]}')
"

echo "=== Creating Connections ==="
connect $APPLE $THUMB '["success"]' "Apple→Thumbnail"
connect $IPHONE $THUMB '["success"]' "iPhone→Thumbnail"
connect $TAKEOUT $THUMB '["success"]' "Takeout→Thumbnail"
connect $THUMB $SHACL '["success"]' "Thumbnail→SHACL"
connect $SHACL $FUSEKI '["success"]' "SHACL→Fuseki"

echo "=== Pipeline configured ==="
