#!/usr/bin/env python3
"""Generate thumbnails on Bedroom from Takeout source files.
Output to /Volumes/Gathering/Photos/generated/thumbnails/ per CSC convention.
Card: #1644
"""
import json, os, subprocess, sys, time

JOBS = "/tmp/bedroom-sips-jobs.tsv"
THUMB_BASE = "/Volumes/VideosNew/Gathering/Photos/generated/thumbnails"

generated = 0
errors = 0
skipped = 0
t0 = time.time()

with open(JOBS) as f:
    lines = [l.strip().split("\t") for l in f if l.strip()]

print(f"Processing {len(lines)} jobs...", file=sys.stderr, flush=True)

for i, parts in enumerate(lines):
    if len(parts) != 2:
        continue
    src, orig_out = parts
    # Rewrite output to CSC-compliant path
    if "/thumbnails/photos/" in orig_out:
        rel = orig_out.split("/thumbnails/photos/")[1]
        out = os.path.join(THUMB_BASE, rel)
    else:
        continue

    if os.path.exists(out):
        skipped += 1
        continue

    if not os.path.exists(src):
        errors += 1
        continue

    os.makedirs(os.path.dirname(out), exist_ok=True)
    try:
        subprocess.run(["sips", "-Z", "400", "--setProperty", "format", "jpeg", src, "--out", out],
                      capture_output=True, timeout=15)
        if os.path.exists(out):
            generated += 1
        else:
            errors += 1
    except:
        errors += 1

    if (generated + errors) % 1000 == 0 and generated > 0:
        elapsed = time.time() - t0
        rate = generated / elapsed
        print(f"  {generated:,} generated, {errors} errors, {skipped} skipped ({rate:.0f}/s)", file=sys.stderr, flush=True)

elapsed = time.time() - t0
print(f"\nDone in {elapsed:.0f}s: {generated:,} generated, {errors} errors, {skipped} skipped", file=sys.stderr)
