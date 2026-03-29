#!/bin/bash
# NiFi ExecuteStreamCommand — generate thumbnail for a canonical photo record
# Reads merged JSON from stdin, resolves source file, generates 400px thumbnail,
# outputs JSON with thumbnailPath added.
#
# Runs on Bedroom via NiFi. Passthrough on failure (thumbnailPath=null).
# Card: #1644

set -euo pipefail

THUMB_BASE="/Volumes/Gathering/Photos/generated/thumbnails"
TAKEOUT_BASE="/Volumes/VideosNew/Gathering/Photos/GoogleTakeoutPhotos/extracted/Takeout/Google Photos"
APPLE_ORIG="$HOME/Pictures/Photos Library.photoslibrary/originals"

# Read merged JSON from stdin
RECORD=$(cat)

# Extract fields
FILENAME=$(echo "$RECORD" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r.get('filename',{}).get('value','') if isinstance(r.get('filename'),dict) else r.get('filename',''))" 2>/dev/null)
DATE=$(echo "$RECORD" | python3 -c "import sys,json; r=json.load(sys.stdin); v=r.get('g_date',r.get('dateTaken',{})); print(v.get('value','') if isinstance(v,dict) else (v or ''))" 2>/dev/null)

if [ -z "$FILENAME" ]; then
    echo "$RECORD"
    exit 0
fi

# Date bucket
BUCKET="${DATE:0:7}"
[ -z "$BUCKET" ] && BUCKET="unknown"

# Thumbnail output path
THUMB_DIR="$THUMB_BASE/$BUCKET"
THUMB_PATH="$THUMB_DIR/$FILENAME.jpg"

# Skip if exists
if [ -f "$THUMB_PATH" ]; then
    echo "$RECORD" | python3 -c "
import sys,json
r=json.load(sys.stdin)
r['thumbnailPath']='$THUMB_PATH'
json.dump(r,sys.stdout)
"
    exit 0
fi

# Find source file — try Takeout first (larger collection), then Apple originals
SRC=""
if [ -z "$SRC" ] || [ ! -f "$SRC" ]; then
    SRC=$(find "$TAKEOUT_BASE" -name "$FILENAME" -type f 2>/dev/null | head -1)
fi
if [ -z "$SRC" ] || [ ! -f "$SRC" ]; then
    SRC=$(find "$APPLE_ORIG" -name "$FILENAME" -type f 2>/dev/null | head -1)
fi

# No source file — passthrough with null thumbnailPath
if [ -z "$SRC" ] || [ ! -f "$SRC" ]; then
    echo "$RECORD"
    exit 0
fi

# Skip videos
EXT="${FILENAME##*.}"
EXT_LOWER=$(echo "$EXT" | tr '[:upper:]' '[:lower:]')
case "$EXT_LOWER" in
    mov|mp4|m4v|avi|3gp)
        echo "$RECORD"
        exit 0
        ;;
esac

# Generate thumbnail
mkdir -p "$THUMB_DIR"
if sips -Z 400 --setProperty format jpeg "$SRC" --out "$THUMB_PATH" > /dev/null 2>&1 && [ -f "$THUMB_PATH" ]; then
    echo "$RECORD" | python3 -c "
import sys,json
r=json.load(sys.stdin)
r['thumbnailPath']='$THUMB_PATH'
json.dump(r,sys.stdout)
"
else
    # sips failed — passthrough
    echo "$RECORD"
fi
