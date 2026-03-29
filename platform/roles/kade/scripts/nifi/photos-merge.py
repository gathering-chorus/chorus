"""
NiFi ExecuteScript (Python) — Photos era-scoped merge.

Receives a single SPARQL binding (JSON) as flowfile content.
Outputs merged canonical record as N-Triples.

Card: #1644 | Spec: architect/docs/merge-specification-photos.html
"""

import json
import sys
from java.io import BufferedReader, InputStreamReader, BufferedWriter, OutputStreamWriter
from org.apache.nifi.processor.io import StreamCallback

class MergeCallback(StreamCallback):
    def process(self, inputStream, outputStream):
        reader = BufferedReader(InputStreamReader(inputStream, "UTF-8"))
        lines = []
        line = reader.readLine()
        while line is not None:
            lines.append(line)
            line = reader.readLine()
        reader.close()

        record = json.loads("\n".join(lines))
        triples = merge_record(record)

        writer = BufferedWriter(OutputStreamWriter(outputStream, "UTF-8"))
        for triple in triples:
            writer.write(triple)
            writer.newLine()
        writer.flush()
        writer.close()


# ── Era definitions ──────────────────────────────────────────

ERAS = [
    {"name": "pre-digital",    "min": "0001-01-01", "max": "2006-01-01", "golden": "apple"},
    {"name": "camera-era",     "min": "2006-01-01", "max": "2013-01-01", "golden": "apple"},
    {"name": "iphone-primary", "min": "2013-01-01", "max": "2020-01-01", "golden": "apple"},
    {"name": "modern",         "min": "2020-01-01", "max": "2099-01-01", "golden": "iphone"},
]

# Metadata ceilings: (era, source) → set of ceilinged fields
CEILINGS = {
    ("pre-digital", "apple"): {"location", "faces", "sceneLabels"},
    ("pre-digital", "takeout"): {"location", "dimensions"},
    ("camera-era", "takeout"): {"dimensions", "fileSize"},
    ("iphone-primary", "takeout"): {"dimensions", "fileSize"},
    ("modern", "takeout"): {"dimensions", "fileSize"},
    ("modern", "apple"): {"faces"},
}

# Location override: era → ordered preference list for GPS
LOCATION_OVERRIDE = {
    "pre-digital": [],  # no GPS possible
    "camera-era": ["takeout", "apple"],  # Takeout preferred
    "iphone-primary": ["iphone", "apple", "takeout"],  # iPhone preferred
    "modern": ["iphone", "takeout", "apple"],  # iPhone golden
}

# Authority chain: era → ordered source preference
AUTHORITY = {
    "pre-digital": ["apple", "takeout", "iphone"],
    "camera-era": ["apple", "takeout", "iphone"],
    "iphone-primary": ["apple", "iphone", "takeout"],
    "modern": ["iphone", "takeout", "apple"],
}

ERA_URIS = {
    "pre-digital": "icd/era/photos/pre-digital",
    "camera-era": "icd/era/photos/camera-era",
    "iphone-primary": "icd/era/photos/iphone-primary",
    "modern": "icd/era/photos/modern",
}


def val(record, key):
    """Extract value from SPARQL binding, handling nested {type, value} structure."""
    if key not in record:
        return None
    v = record[key]
    if isinstance(v, dict):
        return v.get("value")
    return v


def classify_era(date_str):
    """Classify a date string into an era."""
    if not date_str:
        return "unknown"
    d = date_str[:10]  # YYYY-MM-DD
    if d < "2006-01-01":
        return "pre-digital"
    if d < "2013-01-01":
        return "camera-era"
    if d < "2020-01-01":
        return "iphone-primary"
    return "modern"


def is_ceilinged(era, source, field):
    """Check if a field has a metadata ceiling for this source in this era."""
    key = (era, source)
    if key in CEILINGS:
        # Map canonical field names to ceiling categories
        if field in ("lat", "lon", "location"):
            return "location" in CEILINGS[key]
        if field in ("width", "height", "dimensions"):
            return "dimensions" in CEILINGS[key]
        if field == "fileSize":
            return "fileSize" in CEILINGS[key]
        if field == "faces":
            return "faces" in CEILINGS[key]
    return False


def era_precedence(record, field_name, source_keys, era):
    """Get value using era-precedence strategy with ceiling filtering."""
    for source, key in source_keys:
        if is_ceilinged(era, source, field_name):
            continue
        v = val(record, key)
        if v is not None and v != "":
            # Date rejection gate: within 7 days of harvest = suspect
            if field_name == "dateTaken" and is_near_harvest(v):
                continue
            # Apple sentinel GPS filter
            if field_name in ("lat", "lon") and v == "-180.0":
                continue
            return v, source
    return None, None


def is_near_harvest(date_str):
    """Check if date is within 7 days of known harvest dates (Takeout bug)."""
    if not date_str:
        return False
    d = date_str[:10]
    harvest_dates = ["2026-03-12", "2025-08-02", "2026-03-22", "2026-03-23"]
    for hd in harvest_dates:
        # Simple check: same week
        if abs(ord(d[8]) - ord(hd[8])) <= 7 and d[:7] == hd[:7]:
            return True
    return False


def escape_ntriples(s):
    """Escape a string for N-Triples literal."""
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r")


def merge_record(record):
    """Merge a single SPARQL binding into canonical N-Triples."""
    filename = val(record, "filename")
    if not filename:
        return []

    # Golden source date determines the era
    golden_date = val(record, "g_date")
    era = classify_era(golden_date)

    if era == "unknown":
        # No date → dead letter (handled by NiFi routing)
        return [f'# DEAD-LETTER: no date for {filename}']

    authority = AUTHORITY.get(era, ["apple", "takeout", "iphone"])
    loc_chain = LOCATION_OVERRIDE.get(era, authority)

    # Source prefix mapping for SPARQL variables
    source_map = {
        "apple": "g" if era != "modern" else "a",  # golden varies by era
        "takeout": "t",
        "iphone": "i",
    }
    # In our queries, golden is always "g_", supplementary are first-letter prefixed
    # For eras 1-3: g=apple, i=iphone, t=takeout
    # For era 4 (modern): g=iphone, a=apple, t=takeout

    # Build source key lists for era-precedence
    def field_keys(field_suffix):
        """Return [(source_name, sparql_var_key), ...] in authority order."""
        keys = []
        for src in authority:
            if era == "modern":
                prefix = {"iphone": "g", "apple": "a", "takeout": "t"}[src]
            else:
                prefix = {"apple": "g", "iphone": "i", "takeout": "t"}[src]
            keys.append((src, f"{prefix}_{field_suffix}"))
        return keys

    def loc_keys(field_suffix):
        """Return [(source_name, sparql_var_key), ...] in location override order."""
        keys = []
        for src in loc_chain:
            if era == "modern":
                prefix = {"iphone": "g", "apple": "a", "takeout": "t"}[src]
            else:
                prefix = {"apple": "g", "iphone": "i", "takeout": "t"}[src]
            keys.append((src, f"{prefix}_{field_suffix}"))
        return keys

    # ── Merge each field ──

    # Identity: era-precedence
    date_taken, date_src = era_precedence(record, "dateTaken", field_keys("date"), era)
    dimensions_w, dim_src = era_precedence(record, "width", field_keys("width"), era)
    dimensions_h, _ = era_precedence(record, "height", field_keys("height"), era)
    file_size, _ = era_precedence(record, "fileSize", field_keys("fileSize"), era)
    media_type, _ = era_precedence(record, "mediaType", field_keys("mediaSubtype"), era)

    # Location: era-precedence with override chain
    lat, loc_src = era_precedence(record, "lat", loc_keys("lat"), era)
    lon, _ = era_precedence(record, "lon", loc_keys("lon"), era)

    # Merge-all: hasSourceRecord (accumulate from all sources)
    source_records = []
    for src in ["apple", "iphone", "takeout"]:
        if era == "modern":
            prefix = {"iphone": "g", "apple": "a", "takeout": "t"}[src]
        else:
            prefix = {"apple": "g", "iphone": "i", "takeout": "t"}[src]
        if val(record, f"{prefix}_date") is not None:
            source_records.append(f"{src}:{filename}")

    # ── Generate N-Triples ──

    # Canonical photo URI based on filename (will be replaced by phash in future)
    safe_fn = filename.replace(" ", "_").replace("'", "")
    photo_uri = f"<urn:jb:photos/canonical/{safe_fn}>"
    prefix = "https://jeffbridwell.com/ontology#"

    triples = [
        f'{photo_uri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{prefix}Photo> .',
        f'{photo_uri} <{prefix}photoFilename> "{escape_ntriples(filename)}" .',
    ]

    if date_taken:
        triples.append(f'{photo_uri} <{prefix}dateTaken> "{date_taken}"^^<http://www.w3.org/2001/XMLSchema#dateTime> .')

    if lat and lon:
        triples.append(f'{photo_uri} <{prefix}latitude> "{lat}"^^<http://www.w3.org/2001/XMLSchema#decimal> .')
        triples.append(f'{photo_uri} <{prefix}longitude> "{lon}"^^<http://www.w3.org/2001/XMLSchema#decimal> .')

    if dimensions_w:
        triples.append(f'{photo_uri} <{prefix}imageWidth> "{dimensions_w}"^^<http://www.w3.org/2001/XMLSchema#integer> .')
    if dimensions_h:
        triples.append(f'{photo_uri} <{prefix}imageHeight> "{dimensions_h}"^^<http://www.w3.org/2001/XMLSchema#integer> .')

    if file_size:
        triples.append(f'{photo_uri} <{prefix}fileSize> "{file_size}"^^<http://www.w3.org/2001/XMLSchema#integer> .')

    if media_type:
        triples.append(f'{photo_uri} <{prefix}mediaType> "{escape_ntriples(media_type)}" .')

    # Source records (merge-all)
    for sr in source_records:
        triples.append(f'{photo_uri} <{prefix}hasSourceRecord> "{escape_ntriples(sr)}" .')

    # Provenance
    era_uri = ERA_URIS.get(era, f"icd/era/photos/{era}")
    triples.append(f'{photo_uri} <{prefix}mergeEra> <{era_uri}> .')

    # Field-level provenance for key fields
    if date_src:
        triples.append(f'{photo_uri} <{prefix}dateTakenSource> "{date_src}" .')
    if loc_src:
        triples.append(f'{photo_uri} <{prefix}locationSource> "{loc_src}" .')

    return triples


# ── NiFi entry point ──
flowFile = session.get()
if flowFile is not None:
    flowFile = session.write(flowFile, MergeCallback())
    session.transfer(flowFile, REL_SUCCESS)
