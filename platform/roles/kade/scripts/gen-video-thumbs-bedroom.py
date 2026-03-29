#!/usr/bin/env python3
"""Generate video thumbnails on Bedroom via ffmpeg.
Output to /Volumes/VideosNew/Gathering/Photos/generated/thumbnails/ per CSC.
Card: #1644
"""
import os, subprocess, sys, time

JOBS = "/tmp/bedroom-video-jobs.tsv"
generated = 0
errors = 0
t0 = time.time()

with open(JOBS) as f:
    lines = [l.strip().split("\t") for l in f if l.strip() and "\t" in l]

print(f"Processing {len(lines)} video thumbnails...", file=sys.stderr, flush=True)

for i, (src, out) in enumerate(lines):
    if os.path.exists(out):
        continue
    os.makedirs(os.path.dirname(out), exist_ok=True)
    try:
        subprocess.run(["/opt/homebrew/bin/ffmpeg", "-i", src, "-ss", "00:00:01", "-vframes", "1",
                       "-vf", "scale=400:-1", "-y", out],
                      capture_output=True, timeout=15)
        if os.path.exists(out):
            generated += 1
        else:
            errors += 1
    except:
        errors += 1

    if (generated + errors) % 500 == 0 and generated > 0:
        print(f"  {generated} generated, {errors} errors ({time.time()-t0:.0f}s)", file=sys.stderr, flush=True)

print(f"\nDone in {time.time()-t0:.0f}s: {generated} generated, {errors} errors", file=sys.stderr)
