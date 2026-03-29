#!/bin/bash
# Idempotent Thumbnail Generator v2 (#1744)
#
# Single truth: queries Fuseki canonical graph for canonicalId, resolves source
# image from Apple/iPhone/Takeout, generates thumbnail named {canonicalId}.jpg.
#
# No independent UUID computation. The merge owns the UUID, this script uses it.
#
# Idempotent: run 10 times, same result. Skip existing, create missing.
#
# Usage: generate-thumbnails-v2.sh [batch_size] [offset]
#   Emits progress to stderr and The Clearing API.

set -euo pipefail

FUSEKI="http://localhost:3030/pods/sparql"
THUMB_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/thumbnails/photos"
APPLE_DERIV="$HOME/Pictures/Photos Library.photoslibrary/resources/derivatives"
APPLE_ORIG="$HOME/Pictures/Photos Library.photoslibrary/originals"
THUMB_SIZE=200
BATCH=${1:-5000}
OFFSET=${2:-0}
CLEARING_API="http://localhost:3470/api/message"

emit() {
  echo "[$(date +%H:%M:%S)] $1" >&2
  curl -s -X POST "$CLEARING_API" -H 'Content-Type: application/json' \
    -d "{\"from\":\"silas\",\"text\":\"[batch] thumbnails: $1\"}" > /dev/null 2>&1 || true
}

emit "Starting batch=$BATCH offset=$OFFSET"

# Query canonical records — get canonicalId, filename, dateTaken, sourceSystems
RECORDS=$(curl -s "$FUSEKI" -H 'Content-Type: application/sparql-query' -H 'Accept: application/json' \
  -d "PREFIX jb: <http://jeffbridwell.com/ontology#>
SELECT ?canonId ?fn ?date ?sources WHERE {
  GRAPH <urn:gathering:photos/canonical> {
    ?s jb:canonicalId ?canonId ; jb:filename ?fn ; jb:dateTaken ?date ; jb:sourceSystems ?sources .
  }
} ORDER BY ?date LIMIT $BATCH OFFSET $OFFSET")

echo "$RECORDS" | python3 -c "
import json, sys, subprocess, os

data = json.load(sys.stdin)
records = data['results']['bindings']
thumb_dir = '$THUMB_DIR'
apple_deriv = '$APPLE_DERIV'
apple_orig = '$APPLE_ORIG'
thumb_size = $THUMB_SIZE

generated = 0
skipped = 0
failed = 0
total = len(records)

for i, r in enumerate(records):
    canon_id = r['canonId']['value']
    fn = r['fn']['value']
    date = r['date']['value']
    sources = r['sources']['value']
    bucket = date[:7] if len(date) >= 7 else 'unknown'

    out_dir = os.path.join(thumb_dir, bucket)
    out_file = os.path.join(out_dir, f'{canon_id}.jpg')

    # Idempotent: skip if exists
    if os.path.exists(out_file):
        skipped += 1
        continue

    os.makedirs(out_dir, exist_ok=True)
    source_file = None
    fn_base = fn.rsplit('.', 1)[0].upper() if '.' in fn else fn.upper()

    # Strategy 1: Apple derivatives (fn is Apple UUID for apple-sourced records)
    if 'apple' in sources:
        first_char = fn_base[0] if fn_base else '0'
        for pat in [f'{fn_base}_4_5005_c.jpeg', f'{fn_base}_1_105_c.jpeg', f'{fn_base}_1_102_o.jpeg']:
            candidate = os.path.join(apple_deriv, first_char, pat)
            if os.path.exists(candidate):
                source_file = candidate
                break
        # Fallback: Apple originals
        if not source_file:
            for ext in ['.jpeg', '.heic', '.jpg', '.png', '.HEIC', '.JPG']:
                candidate = os.path.join(apple_orig, first_char, f'{fn_base}{ext}')
                if os.path.exists(candidate):
                    source_file = candidate
                    break

    # Strategy 2: iPhone — source is Apple Photos SQLite (same derivatives/originals)
    # iPhone photos imported to Photos Library have Apple UUIDs too
    if not source_file and 'iphone' in sources:
        first_char = fn_base[0] if fn_base else '0'
        for pat in [f'{fn_base}_4_5005_c.jpeg', f'{fn_base}_1_105_c.jpeg']:
            candidate = os.path.join(apple_deriv, first_char, pat)
            if os.path.exists(candidate):
                source_file = candidate
                break
        if not source_file:
            for ext in ['.jpeg', '.heic', '.jpg', '.png', '.HEIC', '.JPG', '.mov', '.MOV']:
                candidate = os.path.join(apple_orig, first_char, f'{fn_base}{ext}')
                if os.path.exists(candidate):
                    source_file = candidate
                    break

    # Strategy 3: Takeout — files on Bedroom NFS (skip for now, run separately on Bedroom)

    if source_file:
        try:
            result = subprocess.run(
                ['sips', '-Z', str(thumb_size), source_file, '--out', out_file],
                capture_output=True, timeout=15
            )
            if result.returncode == 0 and os.path.exists(out_file):
                generated += 1
            else:
                failed += 1
        except Exception:
            failed += 1
    else:
        failed += 1

    # Progress every 500
    if (i + 1) % 500 == 0:
        msg = f'{generated} new, {skipped} exist, {failed} no source ({i+1}/{total})'
        print(f'[progress] {msg}', file=sys.stderr, flush=True)

msg = f'Done: {generated} generated, {skipped} existed, {failed} no source'
print(msg, file=sys.stderr)
print(json.dumps({'generated': generated, 'skipped': skipped, 'failed': failed, 'total': total}))
"
