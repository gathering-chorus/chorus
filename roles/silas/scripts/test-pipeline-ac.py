#!/usr/bin/env python3
"""
Pipeline AC verification tests — run after pipeline completes.
Card: #1705
"""
import json, os, subprocess, sys

FUSEKI = "http://localhost:3030/pods/query"
GRAPH = "urn:gathering:photos/canonical"
CSC_THUMB = "/Volumes/Gathering-1/Photos/generated/thumbnails"
DB_PATH = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite")

def sparql(query):
    r = subprocess.run(
        ["curl", "-s", FUSEKI, "-G", "--data-urlencode", f"query={query}",
         "-H", "Accept: application/sparql-results+json"],
        capture_output=True, text=True
    )
    return json.loads(r.stdout)["results"]["bindings"]

def test(name, passed, detail=""):
    status = "PASS" if passed else "FAIL"
    print(f"  [{status}] {name}" + (f" — {detail}" if detail else ""))
    return passed

results = []

# ============================================================
print("=== AC Test 1: SHACL rejects records missing dateTaken ===")
# Check: no records in canonical without dateTaken
rows = sparql(f"""
    SELECT (COUNT(*) AS ?count) WHERE {{
        GRAPH <{GRAPH}> {{
            ?s a <https://jeffbridwell.com/ontology#Photo> .
            FILTER NOT EXISTS {{ ?s <https://jeffbridwell.com/ontology#dateTaken> ?d }}
        }}
    }}
""")
no_date = int(rows[0]["count"]["value"])
results.append(test("No records without dateTaken in canonical", no_date == 0, f"{no_date} found"))

# ============================================================
print("=== AC Test 2: SHACL rejects records missing thumbnailPath ===")
rows = sparql(f"""
    SELECT (COUNT(*) AS ?count) WHERE {{
        GRAPH <{GRAPH}> {{
            ?s a <https://jeffbridwell.com/ontology#Photo> .
            FILTER NOT EXISTS {{ ?s <https://jeffbridwell.com/ontology#thumbnailPath> ?t }}
        }}
    }}
""")
no_thumb = int(rows[0]["count"]["value"])
results.append(test("No records without thumbnailPath in canonical", no_thumb == 0, f"{no_thumb} found"))

# ============================================================
print("=== AC Test 3: Canonical count vs source count ===")
import sqlite3
conn = sqlite3.connect(DB_PATH)
source_count = conn.execute("""
    SELECT COUNT(*) FROM ZASSET a
    JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
    WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL
""").fetchone()[0]
conn.close()

rows = sparql(f"""
    SELECT (COUNT(*) AS ?count) WHERE {{
        GRAPH <{GRAPH}> {{ ?s a <https://jeffbridwell.com/ontology#Photo> }}
    }}
""")
canonical_count = int(rows[0]["count"]["value"])
rejection_rate = 100 * (1 - canonical_count / source_count) if source_count > 0 else 0
results.append(test(
    f"Canonical count reasonable (source={source_count}, canonical={canonical_count})",
    canonical_count > 0 and rejection_rate < 50,
    f"rejection rate={rejection_rate:.1f}%"
))

# ============================================================
print("=== AC Test 4: Sentinel lat/lon filtered ===")
rows = sparql(f"""
    SELECT (COUNT(*) AS ?count) WHERE {{
        GRAPH <{GRAPH}> {{
            ?s <https://jeffbridwell.com/ontology#latitude> ?lat .
            FILTER(?lat = -180.0 || ?lat = 180.0)
        }}
    }}
""")
sentinel_count = int(rows[0]["count"]["value"])
results.append(test("No sentinel lat/lon values (-180) in canonical", sentinel_count == 0, f"{sentinel_count} found"))

# ============================================================
print("=== AC Test 5: Every canonical record has required fields ===")
rows = sparql(f"""
    SELECT (COUNT(*) AS ?total)
           (SUM(IF(BOUND(?fn), 1, 0)) AS ?has_fn)
           (SUM(IF(BOUND(?dt), 1, 0)) AS ?has_dt)
           (SUM(IF(BOUND(?src), 1, 0)) AS ?has_src)
           (SUM(IF(BOUND(?tp), 1, 0)) AS ?has_tp)
           (SUM(IF(BOUND(?uid), 1, 0)) AS ?has_uid)
    WHERE {{
        GRAPH <{GRAPH}> {{
            ?s a <https://jeffbridwell.com/ontology#Photo> .
            OPTIONAL {{ ?s <https://jeffbridwell.com/ontology#photoFilename> ?fn }}
            OPTIONAL {{ ?s <https://jeffbridwell.com/ontology#dateTaken> ?dt }}
            OPTIONAL {{ ?s <https://jeffbridwell.com/ontology#source> ?src }}
            OPTIONAL {{ ?s <https://jeffbridwell.com/ontology#thumbnailPath> ?tp }}
            OPTIONAL {{ ?s <https://jeffbridwell.com/ontology#uuid> ?uid }}
        }}
    }}
""")
if rows:
    r = rows[0]
    total = int(r["total"]["value"])
    for field in ["fn", "dt", "src", "tp", "uid"]:
        count = int(r[f"has_{field}"]["value"])
        field_name = {"fn": "filename", "dt": "dateTaken", "src": "source", "tp": "thumbnailPath", "uid": "uuid"}[field]
        results.append(test(f"All records have {field_name}", count == total, f"{count}/{total}"))

# ============================================================
print("=== AC Test 6: Thumbnail files exist on CSC for canonical records ===")
rows = sparql(f"""
    SELECT ?tp WHERE {{
        GRAPH <{GRAPH}> {{
            ?s <https://jeffbridwell.com/ontology#thumbnailPath> ?tp
        }}
    }} LIMIT 50
""")
checked = 0
missing = 0
for row in rows:
    tp = row["tp"]["value"]
    # Convert relative path to CSC absolute path
    # tp looks like /thumbnails/photos/2010-01/uuid.jpg
    csc_path = tp.replace("/thumbnails/photos/", f"{CSC_THUMB}/")
    if not os.path.exists(csc_path):
        missing += 1
    checked += 1
results.append(test(
    f"Thumbnail files exist on CSC (checked {checked})",
    missing == 0,
    f"{missing} missing out of {checked}"
))

# ============================================================
print("\n=== SUMMARY ===")
passed = sum(1 for r in results if r)
total = len(results)
print(f"{passed}/{total} tests passed")
if passed < total:
    print("PIPELINE NOT READY — fix failures before demo")
    sys.exit(1)
else:
    print("ALL TESTS PASSED")
