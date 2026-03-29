#!/bin/bash
# Configure Apple Photos ExecuteSQL processor on NiFi
# Card: #1705
NIFI="https://192.168.86.242:8443/nifi-api"
TOKEN=$(curl -sk "$NIFI/access/token" -d "username=admin&password=nifi-gathering-2026" 2>/dev/null)
PROC="29baf61e-019d-1000-d44c-1ab923817682"
DBCP="29efe61f-019d-1000-6861-7246d90a55fb"

REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/processors/$PROC" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)

SQL="SELECT a.ZUUID as uuid, b.ZORIGINALFILENAME as filename, datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken, a.ZLATITUDE as lat, a.ZLONGITUDE as lon, a.ZPIXELWIDTH as width, a.ZPIXELHEIGHT as height, CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType FROM ZASSET a JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL"

python3 -c "
import json, subprocess
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$PROC',
        'config': {
            'properties': {
                'Database Connection Pooling Service': '$DBCP',
                'SQL select query': '''$SQL'''
            },
            'schedulingPeriod': '0 sec',
            'schedulingStrategy': 'TIMER_DRIVEN',
            'autoTerminatedRelationships': ['failure']
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', 'Authorization: Bearer $TOKEN',
    '-H', 'Content-Type: application/json',
    '$NIFI/processors/$PROC',
    '-d', json.dumps(payload)],
    capture_output=True, text=True)
d = json.loads(r.stdout)
print(f'Configured: {d[\"component\"][\"name\"]} — rev {d[\"revision\"][\"version\"]}')
"
