#!/bin/bash
# Generate thumbnails for canonical photos — maps canonical UUID to source image files
# Runs on Library, outputs to Bedroom via SSH
#
# Three source strategies:
# 1. Apple derivatives: ~/Pictures/Photos Library.photoslibrary/resources/derivatives/{UUID[0]}/{UUID}_4_5005_c.jpeg
# 2. iPhone backup: Manifest.db hash lookup → backup file → sips resize
# 3. Takeout: source files on Bedroom NFS
#
# Output: /Volumes/VideosNew/Gathering/Photos/generated/thumbnails/{YYYY-MM}/{canonical-uuid}.jpg

set -euo pipefail

FUSEKI="http://localhost:3030/pods/sparql"
THUMB_BASE="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/thumbnails/photos"
APPLE_DERIV="$HOME/Pictures/Photos Library.photoslibrary/resources/derivatives"
IPHONE_BACKUP="$HOME/Library/Application Support/MobileSync/Backup/00008130-000419691AD8001C"
THUMB_SIZE=200
BATCH=${1:-1000}
OFFSET=${2:-0}

echo "[$(date +%H:%M:%S)] Generating thumbnails — batch=$BATCH offset=$OFFSET" >&2

# Query canonical records that need thumbnails
RECORDS=$(curl -s "$FUSEKI" -H 'Content-Type: application/sparql-query' -H 'Accept: application/json' \
  -d "PREFIX jb: <http://jeffbridwell.com/ontology#>
SELECT ?canonId ?fn ?date ?sources ?thumbPath WHERE {
  GRAPH <urn:gathering:photos/canonical> {
    ?s jb:canonicalId ?canonId ; jb:filename ?fn ; jb:dateTaken ?date ;
       jb:sourceSystems ?sources ; jb:thumbnailPath ?thumbPath .
  }
} ORDER BY ?date LIMIT $BATCH OFFSET $OFFSET")

TOTAL=$(echo "$RECORDS" | python3 -c "import json,sys; print(len(json.load(sys.stdin)['results']['bindings']))")
echo "[$(date +%H:%M:%S)] Processing $TOTAL records" >&2

echo "$RECORDS" | python3 -c "
import json, sys, subprocess, os

data = json.load(sys.stdin)
records = data['results']['bindings']

apple_deriv = '$APPLE_DERIV'
iphone_backup = '$IPHONE_BACKUP'
thumb_base = '$THUMB_BASE'
generated = 0
skipped = 0
failed = 0

for r in records:
    canon_id = r['canonId']['value']
    fn = r['fn']['value']
    date = r['date']['value']
    sources = r['sources']['value']
    thumb_path = r['thumbPath']['value']

    # Date bucket from thumbPath
    parts = thumb_path.split('/')
    bucket = parts[-2] if len(parts) >= 2 else date[:7]
    out_dir = os.path.join(thumb_base, bucket)
    out_file = os.path.join(out_dir, f'{canon_id}.jpg')

    # Skip if already exists
    if os.path.exists(out_file):
        skipped += 1
        continue

    os.makedirs(out_dir, exist_ok=True)
    source_file = None

    # Strategy 1: Apple derivatives (fn is Apple UUID)
    if 'apple' in sources and fn.endswith(('.jpeg', '.jpg', '.JPG', '.JPEG', '.heic', '.HEIC', '.png', '.PNG')):
        apple_uuid = fn.rsplit('.', 1)[0].upper()
        first_char = apple_uuid[0]
        for pat in [f'{apple_uuid}_4_5005_c.jpeg', f'{apple_uuid}_1_105_c.jpeg', f'{apple_uuid}_1_102_o.jpeg']:
            candidate = os.path.join(apple_deriv, first_char, pat)
            if os.path.exists(candidate):
                source_file = candidate
                break

    # Strategy 1b: Apple originals (full-size, sips will resize)
    if not source_file and 'apple' in sources:
        apple_uuid = fn.rsplit('.', 1)[0].upper()
        first_char = apple_uuid[0]
        for ext in ['.jpeg', '.heic', '.jpg', '.png', '.HEIC', '.JPG', '.mov', '.mp4']:
            candidate = os.path.join('$APPLE_DERIV'.replace('/derivatives', '/originals'), first_char, f'{apple_uuid}{ext}')
            if os.path.exists(candidate):
                source_file = candidate
                break

    # Strategy 2: iPhone backup hash lookup
    if not source_file and 'iphone' in sources:
        # Skip for now — requires Manifest.db query per file, too slow for batch
        pass

    # Strategy 3: Takeout source on Bedroom (NFS mount)
    if not source_file and 'takeout' in sources:
        # Takeout files are on Bedroom — skip in this script (runs on Library)
        pass

    if source_file:
        try:
            subprocess.run(['sips', '-Z', '$THUMB_SIZE', source_file, '--out', out_file],
                         capture_output=True, timeout=10)
            generated += 1
        except Exception as e:
            failed += 1
    else:
        failed += 1

    if (generated + skipped + failed) % 500 == 0:
        print(f'[progress] {generated} generated, {skipped} skipped, {failed} failed', file=sys.stderr)

print(f'Done: {generated} generated, {skipped} existed, {failed} no source', file=sys.stderr)
print(json.dumps({'generated': generated, 'skipped': skipped, 'failed': failed}))
"
