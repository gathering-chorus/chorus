#!/usr/bin/env python3
"""
Generate thumbnails from Apple Photos derivatives on Library Mac.
Reads canonical records from stdin (JSON lines), generates thumbnails,
writes to CSC path via SMB mount.

Runs ON LIBRARY — called by NiFi via SSH.
Card: #1705
"""
import json, os, subprocess, sys

DERIV_BASE = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/resources/derivatives")
ORIG_BASE = os.path.expanduser("~/Pictures/Photos Library.photoslibrary/originals")
CSC_THUMB = "/Volumes/Gathering-1/Photos/generated/thumbnails"
PREFIX = "https://jeffbridwell.com/ontology#"
XSD = "http://www.w3.org/2001/XMLSchema#"

def esc(s):
    return str(s).replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")

def emit_ntriples(record):
    """Emit N-Triples for a validated record."""
    uuid = record["uuid"].lower()
    uri = f"<urn:jb:photos/{uuid}>"
    triples = [
        f'{uri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{PREFIX}Photo> .',
        f'{uri} <{PREFIX}uuid> "{esc(uuid)}"^^<{XSD}string> .',
        f'{uri} <{PREFIX}photoFilename> "{esc(record["filename"])}"^^<{XSD}string> .',
        f'{uri} <{PREFIX}dateTaken> "{esc(record["dateTaken"])}"^^<{XSD}dateTime> .',
        f'{uri} <{PREFIX}source> "apple-photos"^^<{XSD}string> .',
        f'{uri} <{PREFIX}thumbnailPath> "{esc(record["thumbnailPath"])}"^^<{XSD}string> .',
        f'{uri} <{PREFIX}appleUuid> "{esc(record["uuid"])}"^^<{XSD}string> .',
    ]
    for field, pred, dtype in [
        ("width", "imageWidth", "integer"), ("height", "imageHeight", "integer"),
        ("mediaType", "mediaType", "string"),
    ]:
        v = record.get(field)
        if v and str(v) != "0" and str(v) != "None":
            triples.append(f'{uri} <{PREFIX}{pred}> "{esc(v)}"^^<{XSD}{dtype}> .')
    for field, pred in [("lat", "latitude"), ("lon", "longitude")]:
        v = record.get(field)
        if v and str(v) not in ("None", "-180.0", "180.0", "-180", "180"):
            triples.append(f'{uri} <{PREFIX}{pred}> "{esc(v)}"^^<{XSD}decimal> .')
    print("\n".join(triples))

generated = 0
skipped = 0
failed = 0

# Read all stdin — may be JSON array or JSON-lines
raw = sys.stdin.read().strip()
if not raw:
    print("[complete] no input", file=sys.stderr)
    sys.exit(0)

try:
    data = json.loads(raw)
    if isinstance(data, list):
        records = data
    else:
        records = [data]
except json.JSONDecodeError:
    # Try JSON-lines
    records = []
    for line in raw.split("\n"):
        line = line.strip()
        if line:
            try:
                records.append(json.loads(line))
            except:
                pass

for record in records:
    uuid = record.get("uuid", "")
    bucket = record.get("dateTaken", "")[:7]
    if not uuid or not bucket:
        failed += 1
        continue

    out_dir = os.path.join(CSC_THUMB, bucket)
    out_path = os.path.join(out_dir, f"{uuid.lower()}.jpg")

    # Skip if already exists
    if os.path.exists(out_path) and os.path.getsize(out_path) > 0:
        # Emit record with thumbnailPath
        record["thumbnailPath"] = f"/thumbnails/photos/{bucket}/{uuid.lower()}.jpg"
        emit_ntriples(record)
        skipped += 1
        continue

    upper = uuid.upper()
    first = upper[0]
    source = None

    # Try derivatives first (small, fast)
    for pat in [f"{upper}_4_5005_c.jpeg", f"{upper}_1_105_c.jpeg", f"{upper}_1_102_o.jpeg"]:
        p = os.path.join(DERIV_BASE, first, pat)
        if os.path.exists(p):
            source = p
            break

    # Fallback: originals
    if not source:
        for ext in [".heic", ".HEIC", ".jpeg", ".jpg", ".png", ".JPG", ".PNG", ".mov", ".MOV"]:
            p = os.path.join(ORIG_BASE, first, f"{upper}{ext}")
            if os.path.exists(p):
                source = p
                break

    if not source:
        failed += 1
        continue

    os.makedirs(out_dir, exist_ok=True)
    try:
        subprocess.run(
            ["sips", "-s", "format", "jpeg", "-z", "400", "300", source, "--out", out_path],
            capture_output=True, timeout=15
        )
        if os.path.exists(out_path) and os.path.getsize(out_path) > 0:
            record["thumbnailPath"] = f"/thumbnails/photos/{bucket}/{uuid.lower()}.jpg"
            emit_ntriples(record)
            generated += 1
        else:
            failed += 1
    except:
        failed += 1

    if (generated + skipped) % 1000 == 0:
        msg = f"[progress] {generated + skipped} processed, {generated} new, {skipped} existing, {failed} failed"
        print(msg, file=sys.stderr)
        # Write progress file for monitoring
        try:
            with open("/Volumes/Gathering-1/Photos/generated/pipeline-progress.txt", "w") as pf:
                pf.write(f"{generated + skipped}/{len(records)} | gen={generated} skip={skipped} fail={failed}\n")
        except: pass

print(f"[complete] generated={generated} skipped={skipped} failed={failed}", file=sys.stderr)
