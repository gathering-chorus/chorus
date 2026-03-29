#!/usr/bin/env python3
"""
Build NiFi-native iPhone photo extraction flow.
Replaces the wrapper-based approach with native NiFi processors:
  1. ExecuteStreamCommand: SCP Photos.sqlite from Library → Bedroom
  2. ExecuteSQL: Query SQLite via JDBC
  3. ConvertRecord: JSON → N-Triples (via ExecuteScript Python)
  4. InvokeHTTP: PUT to Fuseki

Requires: SQLite JDBC driver at ~/lib/sqlite-jdbc-3.45.3.0.jar
Card: #1652
"""
import json, os, subprocess, sys

NIFI_URL = "https://192.168.86.242:8443"
NIFI_USER = os.environ.get("NIFI_USER", "admin")
NIFI_PASS = os.environ.get("NIFI_PASS", "changeme")
OLD_PG_ID = "2245116e-019d-1000-aa26-4be6c373d44e"
ROOT_PG = "1d5480e7-019d-1000-3e90-4a94ff142786"
FUSEKI_URL = "http://192.168.86.36:3030"
SQLITE_PATH = "/Users/jeffbridwell/data/iphone-photos/Photos.sqlite"
JDBC_JAR = "/Users/jeffbridwell/lib/sqlite-jdbc-3.45.3.0.jar"

def get_token():
    return subprocess.check_output([
        "ssh", "192.168.86.242",
        f"curl -sk -X POST '{NIFI_URL}/nifi-api/access/token' "
        f"-H 'Content-Type: application/x-www-form-urlencoded' "
        f"-d 'username={NIFI_USER}&password={NIFI_PASS}'"
    ], text=True).strip()

def nifi_api(method, path, data=None):
    token = get_token()
    cmd = f"curl -sk -X {method} '{NIFI_URL}/nifi-api{path}' -H 'Authorization: Bearer {token}'"
    if data:
        # Write data to file to avoid shell escaping
        data_str = json.dumps(data)
        subprocess.run(["ssh", "192.168.86.242", f"cat > /tmp/nifi-api-payload.json << 'JSONEOF'\n{data_str}\nJSONEOF"],
                       check=True, capture_output=True)
        cmd += f" -H 'Content-Type: application/json' -d @/tmp/nifi-api-payload.json"
    result = subprocess.check_output(["ssh", "192.168.86.242", cmd], text=True)
    return json.loads(result) if result.strip() else {}

# Step 1: Create the DBCPConnectionPool controller service
print("Creating DBCP controller service for SQLite...")
token = get_token()

# First create a new PG for the native flow
print("Creating process group...")
pg = nifi_api("POST", f"/process-groups/{ROOT_PG}/process-groups", {
    "revision": {"version": 0},
    "component": {
        "name": "iPhone Photo Extraction — NiFi Native (#1652)",
        "position": {"x": 400, "y": 0}
    }
})
pg_id = pg["id"]
print(f"  PG: {pg_id}")

# Create DBCPConnectionPool controller service in the PG
print("Creating SQLite DBCP service...")
cs = nifi_api("POST", f"/process-groups/{pg_id}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.dbcp.DBCPConnectionPool",
        "name": "SQLite — iPhone Photos",
        "properties": {
            "Database Connection URL": f"jdbc:sqlite:{SQLITE_PATH}",
            "Database Driver Class Name": "org.sqlite.JDBC",
            "database-driver-locations": JDBC_JAR
        }
    }
})
cs_id = cs["id"]
print(f"  DBCP service: {cs_id}")

# Enable the controller service
print("Enabling DBCP service...")
nifi_api("PUT", f"/controller-services/{cs_id}/run-status", {
    "revision": cs["revision"],
    "state": "ENABLED"
})

# Step 2: Create processors

# 2a. Pre-stage: SCP Photos.sqlite from Library
print("Creating SCP pre-stage...")
p_scp = nifi_api("POST", f"/process-groups/{pg_id}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "1. Copy Photos.sqlite from Library",
        "position": {"x": 0, "y": 0},
        "config": {
            "schedulingStrategy": "TIMER_DRIVEN",
            "schedulingPeriod": "999999 sec",
            "properties": {
                "Command Path": "/usr/bin/scp",
                "Command Arguments": f"192.168.86.36:/tmp/iphone-photos/Photos.sqlite {SQLITE_PATH}"
            },
            "autoTerminatedRelationships": ["output stream", "nonzero status"]
        }
    }
})
print(f"  SCP: {p_scp.get('id', 'created')}")

# SQL query for photo extraction
sql = """SELECT
  'iphone:' || a.ZUUID as sourceId,
  a.ZUUID as uuid,
  a.ZFILENAME as filename,
  datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken,
  CASE WHEN a.ZLATITUDE != -180.0 AND a.ZLATITUDE != 0 THEN a.ZLATITUDE ELSE NULL END as latitude,
  CASE WHEN a.ZLONGITUDE != -180.0 AND a.ZLONGITUDE != 0 THEN a.ZLONGITUDE ELSE NULL END as longitude,
  a.ZWIDTH as width, a.ZHEIGHT as height,
  CASE a.ZKIND WHEN 0 THEN 'photo' WHEN 1 THEN 'video' ELSE 'unknown' END as mediaType,
  a.ZFAVORITE as isFavorite,
  b.ZORIGINALFILESIZE as fileSize,
  'iphone' as source
FROM ZASSET a
LEFT JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
WHERE a.ZTRASHEDSTATE = 0
ORDER BY a.ZDATECREATED"""

# 2b. ExecuteSQL — query the SQLite database
print("Creating ExecuteSQL processor...")
p_sql = nifi_api("POST", f"/process-groups/{pg_id}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteSQL",
        "name": "2. Extract photos from SQLite",
        "position": {"x": 0, "y": 200},
        "config": {
            "properties": {
                "Database Connection Pooling Service": cs_id,
                "SQL select query": sql
            },
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
print(f"  SQL: {p_sql.get('id', 'created')}")

# 2c. ConvertRecord: Avro → JSON (ExecuteSQL outputs Avro by default)
print("Creating Avro→JSON converter...")
# Need JsonRecordSetWriter and AvroReader controller services
avro_reader = nifi_api("POST", f"/process-groups/{pg_id}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.avro.AvroReader",
        "name": "Avro Reader"
    }
})
nifi_api("PUT", f"/controller-services/{avro_reader['id']}/run-status", {
    "revision": avro_reader["revision"],
    "state": "ENABLED"
})

json_writer = nifi_api("POST", f"/process-groups/{pg_id}/controller-services", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.json.JsonRecordSetWriter",
        "name": "JSON Writer"
    }
})
nifi_api("PUT", f"/controller-services/{json_writer['id']}/run-status", {
    "revision": json_writer["revision"],
    "state": "ENABLED"
})

p_convert = nifi_api("POST", f"/process-groups/{pg_id}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ConvertRecord",
        "name": "3. Avro → JSON",
        "position": {"x": 0, "y": 400},
        "config": {
            "properties": {
                "record-reader": avro_reader["id"],
                "record-writer": json_writer["id"]
            },
            "autoTerminatedRelationships": ["failure"]
        }
    }
})
print(f"  Convert: {p_convert.get('id', 'created')}")

# 2d. ExecuteScript: JSON → N-Triples
print("Creating JSON→NT converter (ExecuteScript)...")
p_nt = nifi_api("POST", f"/process-groups/{pg_id}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.ExecuteStreamCommand",
        "name": "4. JSON → N-Triples",
        "position": {"x": 0, "y": 600},
        "config": {
            "properties": {
                "Command Path": "/usr/bin/python3",
                "Command Arguments": "/Users/jeffbridwell/bin/json-to-nt-iphone.py"
            },
            "autoTerminatedRelationships": ["nonzero status", "original"]
        }
    }
})
print(f"  NT: {p_nt.get('id', 'created')}")

# 2e. InvokeHTTP: PUT to Fuseki
print("Creating Fuseki loader...")
p_load = nifi_api("POST", f"/process-groups/{pg_id}/processors", {
    "revision": {"version": 0},
    "component": {
        "type": "org.apache.nifi.processors.standard.InvokeHTTP",
        "name": "5. Load to Fuseki",
        "position": {"x": 0, "y": 800},
        "config": {
            "properties": {
                "HTTP URL": f"{FUSEKI_URL}/pods/data?graph=urn:jb:photos/source/iphone",
                "HTTP Method": "PUT",
                "Content-Type": "application/n-triples"
            },
            "autoTerminatedRelationships": ["Failure", "No Retry", "Original", "Response", "Retry"]
        }
    }
})
print(f"  Load: {p_load.get('id', 'created')}")

# Step 3: Wire connections
print("\nWiring connections...")
connections = [
    (p_scp, p_sql, ["original"]),
    (p_sql, p_convert, ["success"]),
    (p_convert, p_nt, ["success"]),
    (p_nt, p_load, ["output stream"]),
]

for src, dst, rels in connections:
    src_id = src.get("id", src.get("component", {}).get("id"))
    dst_id = dst.get("id", dst.get("component", {}).get("id"))
    if src_id and dst_id:
        nifi_api("POST", f"/process-groups/{pg_id}/connections", {
            "revision": {"version": 0},
            "component": {
                "source": {"id": src_id, "groupId": pg_id, "type": "PROCESSOR"},
                "destination": {"id": dst_id, "groupId": pg_id, "type": "PROCESSOR"},
                "selectedRelationships": rels
            }
        })
        print(f"  {rels[0]}: wired")

print(f"\nDone. NiFi-native flow in PG {pg_id}")
print("Flow: SCP → ExecuteSQL → Avro→JSON → JSON→NT → Fuseki PUT")
