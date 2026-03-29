#!/usr/bin/env python3
"""
Standalone merge script for NiFi ExecuteStreamCommand.
Reads a single SPARQL binding (JSON) from stdin.
Writes N-Triples to stdout.

Usage: echo '{"filename":{"value":"IMG_001.JPG"}, ...}' | python3 photos-merge-cli.py
"""

import json
import sys

AUTHORITY = {
    "pre-digital": ["apple", "takeout", "iphone"],
    "camera-era": ["apple", "takeout", "iphone"],
    "iphone-primary": ["apple", "iphone", "takeout"],
    "modern": ["iphone", "takeout", "apple"],
}

LOCATION_OVERRIDE = {
    "pre-digital": [],
    "camera-era": ["takeout", "apple"],
    "iphone-primary": ["iphone", "apple", "takeout"],
    "modern": ["iphone", "takeout", "apple"],
}

CEILINGS = {
    ("pre-digital", "apple"): {"location", "faces", "sceneLabels"},
    ("pre-digital", "takeout"): {"location", "dimensions"},
    ("camera-era", "takeout"): {"dimensions", "fileSize"},
    ("iphone-primary", "takeout"): {"dimensions", "fileSize"},
    ("modern", "takeout"): {"dimensions", "fileSize"},
    ("modern", "apple"): {"faces"},
}

ERA_URIS = {
    "pre-digital": "icd/era/photos/pre-digital",
    "camera-era": "icd/era/photos/camera-era",
    "iphone-primary": "icd/era/photos/iphone-primary",
    "modern": "icd/era/photos/modern",
}

HARVEST_DATES = ["2026-03-12", "2025-08-02", "2026-03-22", "2026-03-23"]


def val(record, key):
    if key not in record:
        return None
    v = record[key]
    return v.get("value") if isinstance(v, dict) else v


def classify_era(date_str):
    if not date_str:
        return "unknown"
    d = date_str[:10]
    if d < "2006-01-01":
        return "pre-digital"
    if d < "2013-01-01":
        return "camera-era"
    if d < "2020-01-01":
        return "iphone-primary"
    return "modern"


def is_ceilinged(era, source, field):
    key = (era, source)
    if key not in CEILINGS:
        return False
    c = CEILINGS[key]
    if field in ("lat", "lon"):
        return "location" in c
    if field in ("width", "height"):
        return "dimensions" in c
    return field in c


def is_near_harvest(date_str):
    if not date_str:
        return False
    d = date_str[:10]
    for hd in HARVEST_DATES:
        if d[:7] == hd[:7] and abs(int(d[8:10]) - int(hd[8:10])) <= 7:
            return True
    return False


def era_precedence(record, field_name, source_keys, era):
    for source, key in source_keys:
        if is_ceilinged(era, source, field_name):
            continue
        v = val(record, key)
        if v is not None and v != "":
            if field_name == "dateTaken" and source == "takeout" and is_near_harvest(v):
                continue
            if field_name in ("lat", "lon") and v == "-180.0":
                continue
            return v, source
    return None, None


def escape_nt(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r")


def source_prefix(era, source):
    if era == "modern":
        return {"iphone": "g", "apple": "a", "takeout": "t"}[source]
    return {"apple": "g", "iphone": "i", "takeout": "t"}[source]


def merge_record(record):
    filename = val(record, "filename")
    if not filename:
        return []

    golden_date = val(record, "g_date")
    era = classify_era(golden_date)

    if era == "unknown":
        return [f'# DEAD-LETTER: no date for {filename}']

    authority = AUTHORITY[era]
    loc_chain = LOCATION_OVERRIDE[era]

    def field_keys(suffix):
        return [(src, f"{source_prefix(era, src)}_{suffix}") for src in authority]

    def loc_field_keys(suffix):
        return [(src, f"{source_prefix(era, src)}_{suffix}") for src in loc_chain]

    # Era-precedence fields
    date_taken, date_src = era_precedence(record, "dateTaken", field_keys("date"), era)
    dim_w, _ = era_precedence(record, "width", field_keys("width"), era)
    dim_h, _ = era_precedence(record, "height", field_keys("height"), era)
    file_size, _ = era_precedence(record, "fileSize", field_keys("fileSize"), era)
    media_type, _ = era_precedence(record, "mediaType", field_keys("mediaSubtype"), era)

    # Location with override chain
    lat, loc_src = era_precedence(record, "lat", loc_field_keys("lat"), era)
    lon, _ = era_precedence(record, "lon", loc_field_keys("lon"), era)

    # Merge-all: source records
    source_records = []
    for src in ["apple", "iphone", "takeout"]:
        p = source_prefix(era, src)
        if val(record, f"{p}_date") is not None:
            source_records.append(f"{src}:{filename}")

    # Generate N-Triples
    # URI uses filename + date to handle camera counter resets (IMG_NNNN reuse)
    safe_fn = filename.replace(" ", "_").replace("'", "")
    date_slug = (date_taken or golden_date or "unknown")[:10].replace("-", "")
    uri = f"<urn:jb:photos/canonical/{date_slug}/{safe_fn}>"
    ns = "https://jeffbridwell.com/ontology#"
    xsd = "http://www.w3.org/2001/XMLSchema#"

    triples = [
        f'{uri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{ns}Photo> .',
        f'{uri} <{ns}photoFilename> "{escape_nt(filename)}" .',
    ]

    if date_taken:
        triples.append(f'{uri} <{ns}dateTaken> "{date_taken}"^^<{xsd}dateTime> .')
    if lat:
        triples.append(f'{uri} <{ns}latitude> "{lat}"^^<{xsd}decimal> .')
    if lon:
        triples.append(f'{uri} <{ns}longitude> "{lon}"^^<{xsd}decimal> .')
    if dim_w:
        triples.append(f'{uri} <{ns}imageWidth> "{dim_w}"^^<{xsd}integer> .')
    if dim_h:
        triples.append(f'{uri} <{ns}imageHeight> "{dim_h}"^^<{xsd}integer> .')
    if file_size:
        triples.append(f'{uri} <{ns}fileSize> "{file_size}"^^<{xsd}integer> .')
    if media_type:
        triples.append(f'{uri} <{ns}mediaType> "{escape_nt(media_type)}" .')
    for sr in source_records:
        triples.append(f'{uri} <{ns}hasSourceRecord> "{escape_nt(sr)}" .')

    triples.append(f'{uri} <{ns}mergeEra> <{ERA_URIS[era]}> .')

    # thumbnailPath — set by NiFi thumbnail-generation stage, or from record if present
    thumb_path = record.get("thumbnailPath")
    if isinstance(thumb_path, dict):
        thumb_path = thumb_path.get("value")
    if thumb_path:
        triples.append(f'{uri} <{ns}thumbnailPath> "{escape_nt(thumb_path)}" .')

    if date_src:
        triples.append(f'{uri} <{ns}dateTakenSource> "{date_src}" .')
    if loc_src:
        triples.append(f'{uri} <{ns}locationSource> "{loc_src}" .')

    return triples


if __name__ == "__main__":
    record = json.load(sys.stdin)
    triples = merge_record(record)
    for t in triples:
        print(t)
