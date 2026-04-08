# Brief: Sequencing plan review — answers to your 4 questions

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #436

## Sequencing

Plan looks right. Phase 1 dedup before committing to the big sources is the smart move.

## Answers

### 1. Dedup #4 vs #5

XML comparison if #4 has one (it does — 170MB). For #5, enumerate the directory tree (`find . -type f -name '*.mp3' -o -name '*.m4a' | sort`) and compare artist/album/filename paths against #4's XML track listing. If >90% path overlap, #5 is a snapshot of #4 and can skip straight to deprecate. Don't `diff -rq` the full 462GB trees — that's hours of I/O. Path comparison is minutes.

### 2. Source #6 Kirby — no XML

Build a lightweight ID3 enumerator. `ffprobe` (from ffmpeg, already on both Macs) can extract metadata:

```bash
ffprobe -v quiet -print_format json -show_format "$file" | jq '{artist: .format.tags.artist, album: .format.tags.album, title: .format.tags.title}'
```

Walk the directory, extract to JSONL (same format as the XML parser output), feed into the existing transform pipeline. Small script — 19GB / ~2,400 files, should run in minutes.

### 3. Deprecate: archive approach

Neither zip nor rsync. Just `rm -rf` the ToDo staging copy after verification. The data exists in three places: original source, ToDo staging, and Fuseki. Once Fuseki is verified and the original source is still intact, the staging copy is pure redundancy. Archive only if the original source is unreliable (e.g., Source #2 where media is lost). For Source #3's 189GB — delete the staging copy, keep the original on PhotosNew as cold archive.

### 4. Fuseki graph namespace for metadata-only sources

Same namespace (`/pods/jeff/music/`). The ontology should distinguish playable from metadata-only via a property (e.g., `jb:hasMediaFile true/false`), not via graph namespace. Splitting namespaces breaks cross-domain SPARQL queries — you'd need UNIONs everywhere. One namespace, typed data.
