#!/usr/bin/env python3
"""
NiFi ExecuteScript: Apple Photos SQL → Fuseki canonical graph
Reads Avro flowfile from ExecuteSQL, converts each record to N-Triples,
generates thumbnail, validates SHACL fields, POSTs to Fuseki.

Card: #1705
"""
import json, hashlib, os, subprocess, sys

# This runs as a standalone script, not inside NiFi yet.
# Phase 1: read directly from SQLite, write to Fuseki.
# Phase 2: wire into NiFi ExecuteScript processor.

import sqlite3

DB_PATH = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite")
FUSEKI = "http://localhost:3030/pods/data"  # Fuseki is on Library
GRAPH = "urn:gathering:photos/canonical"
DERIV_BASE = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/resources/derivatives")
ORIG_BASE = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/originals")
# CSC path — Bedroom storage mounted on Library via SMB
THUMB_BASE = "/Volumes/Gathering-1/Photos/generated/thumbnails"  # CSC via SMB mount
PREFIX = "https://jeffbridwell.com/ontology#"
XSD = "http://www.w3.org/2001/XMLSchema#"

def make_thumbnail(uuid, deriv_base, orig_base, thumb_base, bucket):
    """Generate thumbnail. Returns path or None."""
    out_dir = os.path.join(thumb_base, bucket)
    out_path = os.path.join(out_dir, f"{uuid.lower()}.jpg")
    if os.path.exists(out_path) and os.path.getsize(out_path) > 0:
        return out_path

    upper = uuid.upper()
    first = upper[0]
    source = None

    # Try derivatives first
    for pat in [f"{upper}_4_5005_c.jpeg", f"{upper}_1_105_c.jpeg"]:
        p = os.path.join(deriv_base, first, pat)
        if os.path.exists(p):
            source = p
            break

    # Fallback: originals
    if not source:
        for ext in [".heic", ".HEIC", ".jpeg", ".jpg", ".png"]:
            p = os.path.join(orig_base, first, f"{upper}{ext}")
            if os.path.exists(p):
                source = p
                break

    if not source:
        return None

    os.makedirs(out_dir, exist_ok=True)
    try:
        subprocess.run(
            ["sips", "-s", "format", "jpeg", "-z", "400", "300", source, "--out", out_path],
            capture_output=True, timeout=15
        )
        if os.path.exists(out_path) and os.path.getsize(out_path) > 0:
            return out_path
    except:
        pass
    return None


def escape_nt(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r")


def main():
    conn = sqlite3.connect(DB_PATH)
    rows = conn.execute("""
        SELECT a.ZUUID, b.ZORIGINALFILENAME,
               datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken,
               a.ZLATITUDE, a.ZLONGITUDE,
               a.ZWIDTH, a.ZHEIGHT,
               CASE WHEN a.ZKIND = 0 THEN 'photo' ELSE 'video' END as mediaType
        FROM ZASSET a
        JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
        WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL
    """).fetchall()
    conn.close()

    print(f"Extracted {len(rows)} records from Photos.sqlite", file=sys.stderr)

    nt_lines = []
    thumbnail_ok = 0
    thumbnail_fail = 0
    shacl_fail = 0

    for zuuid, filename, date_taken, lat, lon, width, height, media_type in rows:
        if not zuuid or not filename or not date_taken:
            shacl_fail += 1
            continue

        uuid = zuuid.lower()
        bucket = date_taken[:7] if date_taken else "unknown"
        uri = f"<urn:jb:photos/{uuid}>"

        # Generate thumbnail
        thumb_path = make_thumbnail(zuuid, DERIV_BASE, ORIG_BASE, THUMB_BASE, bucket)
        if not thumb_path:
            shacl_fail += 1
            continue
        thumbnail_ok += 1

        thumb_rel = f"/thumbnails/photos/{bucket}/{uuid}.jpg"

        # Build N-Triples
        nt_lines.append(f'{uri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{PREFIX}Photo> .')
        nt_lines.append(f'{uri} <{PREFIX}uuid> "{escape_nt(uuid)}"^^<{XSD}string> .')
        nt_lines.append(f'{uri} <{PREFIX}appleUuid> "{escape_nt(zuuid)}"^^<{XSD}string> .')
        nt_lines.append(f'{uri} <{PREFIX}photoFilename> "{escape_nt(filename)}"^^<{XSD}string> .')
        nt_lines.append(f'{uri} <{PREFIX}dateTaken> "{escape_nt(date_taken)}"^^<{XSD}dateTime> .')
        nt_lines.append(f'{uri} <{PREFIX}source> "apple-photos"^^<{XSD}string> .')
        nt_lines.append(f'{uri} <{PREFIX}thumbnailPath> "{escape_nt(thumb_rel)}"^^<{XSD}string> .')

        if media_type:
            nt_lines.append(f'{uri} <{PREFIX}mediaType> "{escape_nt(media_type)}"^^<{XSD}string> .')
        if width and width > 0:
            nt_lines.append(f'{uri} <{PREFIX}imageWidth> "{width}"^^<{XSD}integer> .')
        if height and height > 0:
            nt_lines.append(f'{uri} <{PREFIX}imageHeight> "{height}"^^<{XSD}integer> .')
        if lat and lat != 0:
            nt_lines.append(f'{uri} <{PREFIX}latitude> "{lat}"^^<{XSD}decimal> .')
        if lon and lon != 0:
            nt_lines.append(f'{uri} <{PREFIX}longitude> "{lon}"^^<{XSD}decimal> .')

        if len(nt_lines) % 10000 == 0:
            print(f"  {thumbnail_ok} records processed...", file=sys.stderr)

    print(f"SHACL pass: {thumbnail_ok}, SHACL fail (no thumbnail or missing fields): {shacl_fail}", file=sys.stderr)
    print(f"Total triples: {len(nt_lines)}", file=sys.stderr)

    # Write N-Triples to file
    nt_file = "/Volumes/Gathering-1/Photos/generated/canonical-apple.nt"
    os.makedirs(os.path.dirname(nt_file), exist_ok=True)
    with open(nt_file, "w") as f:
        f.write("\n".join(nt_lines))
    print(f"Written to {nt_file}", file=sys.stderr)

    # Clear existing canonical graph and load new data
    # DELETE
    subprocess.run([
        "curl", "-sk", "-X", "DELETE",
        f"{FUSEKI}?graph={GRAPH}"
    ], capture_output=True)

    # PUT new graph
    result = subprocess.run([
        "curl", "-sk", "-X", "PUT",
        "-H", "Content-Type: application/n-triples",
        f"{FUSEKI}?graph={GRAPH}",
        "--data-binary", f"@{nt_file}"
    ], capture_output=True, text=True)

    print(f"Fuseki response: {result.stdout[:200]}", file=sys.stderr)
    print(f"Done. {thumbnail_ok} canonical records with thumbnails loaded to {GRAPH}", file=sys.stderr)


if __name__ == "__main__":
    main()
