#!/bin/bash
# Fix DBCP service — add SQLite JDBC driver location
NIFI="https://192.168.86.242:8443/nifi-api"
TOKEN=$(curl -sk "$NIFI/access/token" -d "username=admin&password=nifi-gathering-2026" 2>/dev/null)
DBCP="29efe61f-019d-1000-6861-7246d90a55fb"

# Disable
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/controller-services/$DBCP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
echo "Disabling (rev $REV)..."
curl -sk -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    "$NIFI/controller-services/$DBCP/run-status" \
    -d "{\"revision\":{\"version\":$REV},\"state\":\"DISABLED\"}" > /dev/null 2>&1
sleep 3

# Update driver location
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/controller-services/$DBCP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
echo "Updating driver location (rev $REV)..."
python3 -c "
import json, subprocess
payload = {
    'revision': {'version': $REV},
    'component': {
        'id': '$DBCP',
        'properties': {
            'Database Driver Locations': '/opt/homebrew/Cellar/nifi/2.8.0/libexec/lib/sqlite-jdbc-3.45.1.0.jar'
        }
    }
}
r = subprocess.run(['curl', '-sk', '-X', 'PUT',
    '-H', 'Authorization: Bearer $TOKEN',
    '-H', 'Content-Type: application/json',
    '$NIFI/controller-services/$DBCP',
    '-d', json.dumps(payload)], capture_output=True, text=True)
print(r.stdout[:200])
"
sleep 1

# Re-enable
REV=$(curl -sk -H "Authorization: Bearer $TOKEN" "$NIFI/controller-services/$DBCP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"]["version"])' 2>/dev/null)
echo "Enabling (rev $REV)..."
curl -sk -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    "$NIFI/controller-services/$DBCP/run-status" \
    -d "{\"revision\":{\"version\":$REV},\"state\":\"ENABLED\"}" 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(f"State: {d[\"component\"][\"state\"]}")' 2>/dev/null
