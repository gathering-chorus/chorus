#!/bin/bash
NIFI="https://192.168.86.242:8443/nifi-api"
TOKEN=$(curl -sk "$NIFI/access/token" -d "username=admin&password=nifi-gathering-2026" 2>/dev/null)
PROC="29baf61e-019d-1000-d44c-1ab923817682"

# Stop processor first
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/processors/$PROC" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
curl -sk -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    "$NIFI/processors/$PROC/run-status" \
    -d "{\"revision\":{\"version\":$REV},\"state\":\"STOPPED\"}" > /dev/null 2>&1
sleep 2

# Update with correct property name
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/processors/$PROC" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
SQL='SELECT a.ZUUID as uuid, b.ZORIGINALFILENAME as filename, datetime(a.ZDATECREATED + 978307200, '\''unixepoch'\'') as dateTaken, a.ZLATITUDE as lat, a.ZLONGITUDE as lon, a.ZWIDTH as width, a.ZHEIGHT as height, CASE WHEN a.ZKIND = 0 THEN '\''photo'\'' ELSE '\''video'\'' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL'

python3 -c "
import json, subprocess
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$PROC',
        'config': {
            'properties': {
                'SQL Query': '''$SQL'''
            }
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', 'Authorization: Bearer $TOKEN',
    '-H', 'Content-Type: application/json',
    '$NIFI/processors/$PROC',
    '-d', json.dumps(payload)], capture_output=True, text=True)
d = json.loads(r.stdout)
print(f'Updated SQL Query — rev {d[\"revision\"][\"version\"]}')
print(f'SQL Query value: {d[\"component\"][\"config\"][\"properties\"].get(\"SQL Query\", \"NOT SET\")[:80]}')
"

# Restart
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/processors/$PROC" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
curl -sk -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    "$NIFI/processors/$PROC/run-status" \
    -d "{\"revision\":{\"version\":$REV},\"state\":\"RUNNING\"}" > /dev/null 2>&1
echo "Processor restarted"
