#!/usr/bin/env python3
"""
Repeatable photo pipeline — one command, full rebuild.
Card: #1702

Steps (each verified before the next):
1. Extract iPhone from backup → verify count
2. Convert to RDF, load to Fuseki → verify count
3. Canonical rebuild → verify output
4. Load canonical graph to Fuseki
5. Build canonical-index.json with stable UUIDs
6. Generate thumbnails for UUIDs missing files on disk
7. Restart app (manual — prints instruction)

Usage: python3 scripts/photo-pipeline.py [--skip-extract] [--skip-thumbs]
"""

import json
import os
import sys
import subprocess
import hashlib
import time
import urllib.request
import urllib.parse

# #3637 — writes go through the one credential door (#3566): Basic auth when
# FUSEKI_ADMIN_PASSWORD is set, anonymous otherwise (safe until #3630 flips).
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "platform", "scripts")))
from fuseki_auth import write_auth_headers

# --- Config ---
APP_ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "jeff-bridwell-personal-site")
FUSEKI = "http://localhost:3030/pods"
THUMB_BASE = os.path.join(APP_ROOT, "public", "thumbnails", "photos")
INDEX_PATH = os.path.join(APP_ROOT, "data", "pods", "jeff", "photos", "canonical-index.json")
IPHONE_SOURCE = "/tmp/iphone-photos/source-iphone.json"
CANONICAL_NT = "/tmp/canonical-photos-rebuild.nt"

# ICD baselines
ICD_IPHONE = 54543
ICD_APPLE = 24560
ICD_TAKEOUT = 102490

DERIVATIVES_BASE = os.path.join(
    os.environ.get("HOME", "/Users/jeffbridwell"),
    "Pictures/Photos Library.photoslibrary/resources/derivatives"
)
ORIGINALS_BASE = os.path.join(
    os.environ.get("HOME", "/Users/jeffbridwell"),
    "Pictures/Photos Library.photoslibrary/originals"
)


def log(msg):
    print(f"[pipeline] {msg}", file=sys.stderr, flush=True)


def sparql_query(query):
    data = urllib.parse.urlencode({"query": query, "output": "json"}).encode()
    req = urllib.request.Request(f"{FUSEKI}/query", data=data, method="POST")
    req.add_header("Accept", "application/sparql-results+json")
    resp = urllib.request.urlopen(req, timeout=300)
    return json.loads(resp.read())["results"]["bindings"]


def sparql_update(update):
    data = urllib.parse.urlencode({"update": update}).encode()
    req = urllib.request.Request(f"{FUSEKI}/update", data=data, method="POST",
                                 headers=write_auth_headers())
    urllib.request.urlopen(req, timeout=60)


def load_graph(graph_uri, nt_path):
    with open(nt_path, "rb") as f:
        content = f.read()
    url = f"{FUSEKI}/data?graph={urllib.parse.quote(graph_uri, safe='')}"
    req = urllib.request.Request(url, content, method="PUT",
                                 headers=write_auth_headers())
    req.add_header("Content-Type", "application/n-triples")
    resp = urllib.request.urlopen(req, timeout=300)
    return resp.status


def make_uuid(filename, date):
    h = hashlib.md5(f"{filename}|{date}".encode()).hexdigest()
    return f"{h[:8]}-{h[8:12]}-{h[12:16]}-{h[16:20]}-{h[20:]}"


def verify_count(label, actual, expected, tolerance=0.02):
    diff = abs(actual - expected) / max(expected, 1)
    status = "OK" if diff <= tolerance else "WARN"
    log(f"  {label}: {actual} (expected {expected}) [{status}]")
    if diff > 0.10:
        log(f"  ERROR: {label} off by {diff*100:.1f}% — aborting")
        sys.exit(1)


# --- Step 1: Extract iPhone ---
def step_extract():
    log("Step 1: Extracting iPhone photos from backup...")
    harvest_script = os.path.join(APP_ROOT, "scripts", "harvest-iphone-photos.sh")
    result = subprocess.run(
        ["bash", harvest_script, "--output", IPHONE_SOURCE],
        capture_output=True, text=True, cwd=APP_ROOT
    )
    if result.returncode != 0:
        log(f"  FAILED: {result.stderr}")
        sys.exit(1)
    data = json.load(open(IPHONE_SOURCE))
    count = len(data)
    log(f"  Extracted {count} records")
    verify_count("iPhone", count, ICD_IPHONE)
    return count


# --- Step 2: Load iPhone to Fuseki ---
def step_load_iphone():
    log("Step 2: Loading iPhone source graph to Fuseki...")
    data = json.load(open(IPHONE_SOURCE))
    prefix = "https://jeffbridwell.com/ontology#"
    xsd = "http://www.w3.org/2001/XMLSchema#"
    nt_lines = []

    for r in data:
        fn = r.get("filename", "")
        uuid = r.get("uuid", "")
        if not fn or not uuid:
            continue
        uri = f"<urn:gathering:photos/source/iphone/{uuid}>"
        nt_lines.append(f'{uri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{prefix}SourcePhoto> .')
        nt_lines.append(f'{uri} <{prefix}photoFilename> "{fn}" .')

        dt = r.get("dateTaken", "")
        if dt:
            dt_iso = dt.replace(" ", "T") + "Z" if "T" not in dt else dt
            nt_lines.append(f'{uri} <{prefix}dateTaken> "{dt_iso}"^^<{xsd}dateTime> .')
        if r.get("latitude") is not None:
            nt_lines.append(f'{uri} <{prefix}latitude> "{r["latitude"]}"^^<{xsd}decimal> .')
        if r.get("longitude") is not None:
            nt_lines.append(f'{uri} <{prefix}longitude> "{r["longitude"]}"^^<{xsd}decimal> .')
        if r.get("width"):
            nt_lines.append(f'{uri} <{prefix}imageWidth> "{r["width"]}"^^<{xsd}integer> .')
        if r.get("height"):
            nt_lines.append(f'{uri} <{prefix}imageHeight> "{r["height"]}"^^<{xsd}integer> .')
        if r.get("fileSize") and r["fileSize"] > 0:
            nt_lines.append(f'{uri} <{prefix}fileSize> "{r["fileSize"]}"^^<{xsd}integer> .')
        if r.get("mediaType"):
            nt_lines.append(f'{uri} <{prefix}mediaType> "{r["mediaType"]}" .')
        if r.get("mediaSubtype"):
            nt_lines.append(f'{uri} <{prefix}mediaSubtype> "{r["mediaSubtype"]}" .')

    nt_path = "/tmp/iphone-source.nt"
    with open(nt_path, "w") as f:
        f.write("\n".join(nt_lines))

    sparql_update("DROP SILENT GRAPH <urn:gathering:photos/source/iphone>")
    status = load_graph("urn:gathering:photos/source/iphone", nt_path)
    log(f"  Loaded: HTTP {status}, {len(nt_lines)} triples")

    # Verify
    r = sparql_query("SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH <urn:gathering:photos/source/iphone> { ?s a <https://jeffbridwell.com/ontology#SourcePhoto> } }")
    fuseki_count = int(r[0]["c"]["value"])
    verify_count("iPhone in Fuseki", fuseki_count, len(data))


# --- Step 3: Canonical rebuild ---
def step_rebuild():
    log("Step 3: Running canonical rebuild...")
    engineer_dir = os.path.join(os.path.dirname(__file__), "..")
    rebuild_script = os.path.join(os.path.dirname(__file__), "run-canonical-rebuild.py")
    result = subprocess.run(
        ["python3", rebuild_script],
        capture_output=True, text=True,
        cwd=engineer_dir
    )
    if result.returncode != 0:
        log(f"  FAILED: {result.stderr}")
        sys.exit(1)
    # Parse total from stderr
    for line in result.stderr.split("\n"):
        if "Total:" in line:
            log(f"  {line.strip()}")
    log(f"  Output: {CANONICAL_NT}")


# --- Step 4: Load canonical graph ---
def step_load_canonical():
    log("Step 4: Loading canonical graph to Fuseki...")
    sparql_update("DROP SILENT GRAPH <urn:gathering:photos/canonical>")
    status = load_graph("urn:gathering:photos/canonical", CANONICAL_NT)
    size_kb = os.path.getsize(CANONICAL_NT) // 1024
    log(f"  Loaded: HTTP {status} ({size_kb}KB)")


# --- Step 5: Build index ---
def step_build_index():
    log("Step 5: Building canonical-index.json...")
    bindings = sparql_query("""PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?filename ?date ?lat ?lon ?width ?height ?mediaType ?mediaSub ?sourceRecord ?hasLocal ?imagePath ?fileSize
WHERE {
  GRAPH <urn:gathering:photos/canonical> {
    ?s a jb:Photo ; jb:photoFilename ?filename ; jb:dateTaken ?date .
    OPTIONAL { ?s jb:latitude ?lat } OPTIONAL { ?s jb:longitude ?lon }
    OPTIONAL { ?s jb:imageWidth ?width } OPTIONAL { ?s jb:imageHeight ?height }
    OPTIONAL { ?s jb:mediaType ?mediaType } OPTIONAL { ?s jb:mediaSubtype ?mediaSub }
    OPTIONAL { ?s jb:hasSourceRecord ?sourceRecord } OPTIONAL { ?s jb:hasLocalFile ?hasLocal }
    OPTIONAL { ?s jb:imagePath ?imagePath } OPTIONAL { ?s jb:fileSize ?fileSize }
  }
}""")

    # Pass 1: collect sources per photo (multiple rows due to hasSourceRecord OPTIONAL)
    photo_sources = {}
    for b in bindings:
        fn = b.get("filename", {}).get("value", "")
        dt = b.get("date", {}).get("value", "")
        key = f"{fn}|{dt[:10]}"
        sr = b.get("sourceRecord", {}).get("value", "")
        if sr and ":" in sr:
            photo_sources.setdefault(key, set()).add(sr.split(":")[0])

    # Pass 2: build records
    records = []
    seen = set()
    for b in bindings:
        fn = b.get("filename", {}).get("value", "")
        dt = b.get("date", {}).get("value", "")
        key = f"{fn}|{dt[:10]}"
        if key in seen:
            continue
        seen.add(key)

        uuid = make_uuid(fn, dt)
        bucket = dt[:7] if len(dt) >= 7 else "unknown"
        sources = photo_sources.get(key, set())
        if "iphone" in sources:
            src = "iphone"
        elif "apple" in sources:
            src = "apple-photos"
        elif "takeout" in sources:
            src = "google-takeout"
        else:
            src = "unknown"

        rec = {"uuid": uuid, "bucket": bucket, "filename": fn, "date": dt, "source": src}
        if "width" in b:
            rec["width"] = int(b["width"]["value"])
        if "height" in b:
            rec["height"] = int(b["height"]["value"])
        if "lat" in b:
            try:
                rec["lat"] = float(b["lat"]["value"])
            except ValueError:
                pass
        if "lon" in b:
            try:
                rec["lng"] = float(b["lon"]["value"])
            except ValueError:
                pass
        if "mediaType" in b:
            rec["mediaType"] = b["mediaType"]["value"]
        if "mediaSub" in b:
            rec["mediaSub"] = b["mediaSub"]["value"]
        if "hasLocal" in b:
            rec["hasLocalFile"] = b["hasLocal"]["value"] == "true"
        if "imagePath" in b:
            rec["imagePath"] = b["imagePath"]["value"]
        records.append(rec)

    records.sort(key=lambda r: r.get("date", ""), reverse=True)

    with open(INDEX_PATH, "w") as f:
        json.dump(records, f, indent=None)

    newest = records[0]["date"] if records else "none"
    log(f"  {len(records)} records, newest: {newest}")
    return records


# --- Step 6: Generate thumbnails ---
def step_generate_thumbnails(records):
    log("Step 6: Generating thumbnails for records without files on disk...")
    missing = []
    found = 0
    for r in records:
        uuid = r["uuid"]
        bucket = r["bucket"]
        thumb_path = os.path.join(THUMB_BASE, bucket, f"{uuid}.jpg")
        if os.path.exists(thumb_path):
            found += 1
        else:
            # Check if imagePath resolves
            ip = r.get("imagePath", "")
            if ip:
                ip_disk = os.path.join(APP_ROOT, "public", ip.lstrip("/"))
                if os.path.exists(ip_disk):
                    found += 1
                    continue
            missing.append(r)

    log(f"  Already have: {found}, need: {len(missing)}")

    if not missing:
        log("  All thumbnails present!")
        return

    # Build filename → Apple UUID map from Photos.sqlite
    import sqlite3
    import shutil

    photos_db = os.path.join(
        os.environ.get("HOME", "/Users/jeffbridwell"),
        "Pictures/Photos Library.photoslibrary/database/Photos.sqlite"
    )
    apple_map = {}  # (original_filename, date_prefix) → Apple UUID
    if os.path.exists(photos_db):
        try:
            conn = sqlite3.connect(photos_db)
            cur = conn.execute("""
                SELECT a.ZUUID, b.ZORIGINALFILENAME,
                       datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken
                FROM ZASSET a
                JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
                WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL
            """)
            fn_fallback = {}  # separate dict for filename-only fallback
            for row in cur:
                # Key on (filename, date[:10]) to handle iPhone filename reuse
                date_prefix = row[2][:10] if row[2] else ""
                apple_map[(row[1], date_prefix)] = row[0]
                # Always store last-seen as filename-only fallback
                fn_fallback[row[1]] = row[0]
            conn.close()
            log(f"  Apple Photos map: {len(apple_map)} entries (keyed by filename+date)")
        except Exception as e:
            log(f"  Apple Photos SQLite error: {e}")

    generated = 0
    not_found = 0
    for r in missing:
        uuid = r["uuid"]
        bucket = r["bucket"]
        fn = r["filename"]
        out_dir = os.path.join(THUMB_BASE, bucket)
        out_path = os.path.join(out_dir, f"{uuid}.jpg")

        # Look up Apple UUID — filename+date for accuracy, filename-only as fallback
        date_prefix = r.get("date", "")[:10]
        apple_uuid = apple_map.get((fn, date_prefix)) or fn_fallback.get(fn)
        if not apple_uuid:
            not_found += 1
            continue

        upper = apple_uuid.upper()
        first = upper[0]
        source_file = None

        # Check derivatives (small, fast to serve)
        for pat in [f"{upper}_4_5005_c.jpeg", f"{upper}_1_105_c.jpeg"]:
            p = os.path.join(DERIVATIVES_BASE, first, pat)
            if os.path.exists(p):
                source_file = p
                break

        # Fallback: originals
        if not source_file:
            for ext in [".jpeg", ".heic", ".png", ".jpg", ".HEIC", ".JPG", ".PNG"]:
                p = os.path.join(ORIGINALS_BASE, first, f"{upper}{ext}")
                if os.path.exists(p):
                    source_file = p
                    break

        if not source_file:
            not_found += 1
            continue

        # Copy derivative as thumbnail (JPEG derivatives are already web-ready)
        os.makedirs(out_dir, exist_ok=True)
        if source_file.lower().endswith((".jpeg", ".jpg")):
            shutil.copy2(source_file, out_path)
            generated += 1
        else:
            # Convert HEIC/PNG to JPEG via sips
            try:
                subprocess.run(
                    ["sips", "-s", "format", "jpeg", "-Z", "800", source_file, "--out", out_path],
                    capture_output=True, timeout=10
                )
                if os.path.exists(out_path):
                    generated += 1
                else:
                    not_found += 1
            except Exception:
                not_found += 1

        if generated % 1000 == 0 and generated > 0:
            log(f"  Progress: {generated} generated...", )

    log(f"  Generated: {generated}, not available: {not_found}")


# --- Step 7: Verify ---
def step_verify(records):
    log("Step 7: Verifying...")

    # Check thumbnail coverage
    covered = 0
    for r in records:
        uuid = r["uuid"]
        bucket = r["bucket"]
        thumb_path = os.path.join(THUMB_BASE, bucket, f"{uuid}.jpg")
        if os.path.exists(thumb_path):
            covered += 1
            continue
        ip = r.get("imagePath", "")
        if ip:
            ip_disk = os.path.join(APP_ROOT, "public", ip.lstrip("/"))
            if os.path.exists(ip_disk):
                covered += 1

    pct = 100 * covered // max(len(records), 1)
    log(f"  Thumbnail coverage: {covered}/{len(records)} ({pct}%)")
    log(f"  Newest record: {records[0]['date'] if records else 'none'}")

    if pct < 50:
        log("  WARNING: Less than 50% thumbnail coverage")

    log("  To apply: bash ../jeff-bridwell-personal-site/app-state.sh restart")


# --- Main ---
if __name__ == "__main__":
    skip_extract = "--skip-extract" in sys.argv
    skip_thumbs = "--skip-thumbs" in sys.argv

    log("=== Photo Pipeline ===")
    t0 = time.time()

    if not skip_extract:
        step_extract()
        step_load_iphone()
    else:
        log("Step 1-2: Skipped (--skip-extract)")

    step_rebuild()
    step_load_canonical()
    records = step_build_index()

    if not skip_thumbs:
        step_generate_thumbnails(records)
    else:
        log("Step 6: Skipped (--skip-thumbs)")

    step_verify(records)

    elapsed = time.time() - t0
    log(f"=== Pipeline complete in {elapsed:.0f}s ===")
