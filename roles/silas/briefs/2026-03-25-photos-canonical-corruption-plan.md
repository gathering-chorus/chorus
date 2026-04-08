# Photos Canonical Corruption — Assessment and Fix Plan

**Date:** 2026-03-25 | **Author:** Silas | **Cards:** #1644, #1698, #1702

## What's broken

The canonical photo index (`data/pods/jeff/photos/canonical-index.json`) has two corruptions:

### 1. Wrong dates (iPhone source)
- **apple-photos** (27,203 records): 80% date match (24/30 sampled)
- **iphone** (52,887 records): 13% date match (4/30 sampled)
- iPhone dates appear to be extraction/backup timestamps, not EXIF dates
- Result: sort by date shows nonsense order on the photos page

### 2. Wrong UUID→thumbnail mappings (iPhone source)
- Thumbnail files exist for 74,687 records (50,671 iPhone + 24,016 Apple Photos)
- But the EXIF dates in thumbnail files don't match the canonical record they're mapped to
- Example: `IMG_3666.HEIC` canonical says 2025-09-08, thumbnail EXIF says 2022-06-06
- Result: photos page shows wrong images for wrong records

### 3. Source mapping bug (fixed)
- `mapSource()` mapped "iphone" → "google-drive" (fallback). Fixed to "iphone" → "apple".

### 4. Merge contamination (fixed)
- Browse handler merged Google Takeout + Drive photos on top of canonical. Removed — canonical is sole source now.

## Inventory (verified)

| Source | Records | With thumbnail file | Date accuracy |
|--------|---------|-------------------|---------------|
| apple-photos | 27,203 | 24,016 (88%) | ~80% correct |
| iphone | 52,887 | 50,671 (95%) | ~13% correct |
| **Total** | **80,090** | **74,687** | — |

## Fix plan

### Step 1: Verify apple-photos source is trustworthy
- Spot-check 50 apple-photos records: canonical date vs EXIF vs Apple Photos SQLite DB
- If 90%+ match: apple-photos is our clean baseline
- Testable: run check, report match rate

### Step 2: Diagnose iPhone date source
- Where does the iPhone extraction pipeline read dates from?
- Check: is it using file modification time (backup date) instead of EXIF?
- Find the extraction code and identify the bug
- Testable: trace one record from iPhone backup → canonical

### Step 3: Diagnose UUID→thumbnail mismatch
- How are thumbnails generated? What file does the pipeline read to create the thumbnail?
- Is the UUID generated from the same inputs each time?
- Check: domain-context-photos.md UUID function — is the pipeline using it?
- Testable: for one UUID, trace source file → UUID computation → thumbnail generation

### Step 4: Rebuild canonical with correct dates
- Extract EXIF dates from actual source files (Apple Photos SQLite + iPhone backup)
- Regenerate canonical index with real dates
- Verify: 50 random records, canonical date matches EXIF
- Testable: spot-check passes at 95%+

### Step 5: Regenerate thumbnails with stable UUIDs
- Use the pinned UUID function from domain-context-photos.md
- Generate thumbnails from correct source files
- Verify: 50 random records, thumbnail EXIF matches canonical date
- Testable: spot-check passes at 95%+

### Step 6: Verify the page
- Load /photos page 1, 5, 10, 20
- Check: dates descending, thumbnails match visible content, no NO PREVIEW on pages with data
- Jeff verifies — not pipeline output, not curl, not log counts
- Testable: Jeff says "that looks right"

## What NOT to do
- Don't build new endpoints until the data is correct
- Don't test pipelines against their own output
- Don't claim "93% coverage" without verifying thumbnails match
- Don't touch the browse handler or template — the code is fine, the data is wrong
