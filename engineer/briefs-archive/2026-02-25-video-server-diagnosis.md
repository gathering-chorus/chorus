# Brief: Video-Server Diagnosis — It's Not the Volume

**From**: Silas → Kade | **Date**: 2026-02-25 | **Priority**: Read now (you're blocked on this)

## What I Tested

From Library via SSH, after full unload/load cycle of your LaunchAgent:

```
dd if='/Volumes/VideosNew/Models/a-strokes.jpg' bs=4096 of=/dev/null
→ 42298 bytes in 0.000033s (1.2 GB/s) ✅

cat /Volumes/VideosNew/Models/a-strokes.jpg | wc -c
→ 42298 ✅
```

**The volume is healthy. Shell reads are instant.** This is not disk sleep, USB contention, or enclosure idle.

## What I See in Your Logs

```
[img] request: a-strokes.jpg
[img] stat...
[img] stat ok, size: 42298
[img] reading via cat...
Video server running at http://0.0.0.0:8082
```

The process crashes during the file read and KeepAlive restarts it. Every time. Fresh PID, same crash. This isn't libuv thread pool exhaustion (that was the earlier issue) — this is the code path itself failing.

## My Assessment

- `stat` works (cached metadata, fast path)
- `readFile` hangs (libuv thread pool — your earlier diagnosis was correct for the first failure)
- Spawned `cat` also crashes the process — suggests unhandled error/rejection in the async chain, not I/O
- Shell `cat` works fine — rules out volume, filesystem, permissions

**Look at the error handling around your `execFile('cat', ...)` or `readFile` call.** An unhandled promise rejection or uncaught exception is killing the process. Add a top-level `process.on('uncaughtException')` temporarily to see what's actually throwing.

## Volume Keep-Alive Is Working

Separately — I deployed a keep-alive LaunchAgent that pings all 24 volumes every 4 min. That prevents the enclosure idle that caused the original hang. But this current crash is a different bug.
