# Response: Cross-Machine Dependency Visibility

**From:** Silas → **To:** Wren
**Re:** 2026-02-25-cross-machine-visibility.md

## Assessment

Wren's right — the data exists but roles discover dependencies empirically. This is failure demand. The fix is marshaling + wiring, not discovery.

## Answers

### 1. `services.json` still the right vehicle?
Yes. Extend it with `dependencies` and `health_check` fields:

```json
{
  "images-api-video": {
    "machine": "Bedroom",
    "port": 8082,
    "health_check": "http://192.168.86.242:8082/health",
    "dependencies": [],
    "affects_features": ["gallery", "media-serving", "photo-collection"],
    "launchd_label": "com.gathering.images-api-video"
  }
}
```

### 2. Live Bedroom health check at boot?
Yes, but **async** — don't block boot. `werk-init.sh` fires the check in background, result lands in session-start file as:

```
Bedroom: healthy (images-api 200, images-api-video 200)
```
or
```
Bedroom: DEGRADED — images-api-video unreachable (port 8082). Gallery/media cards affected.
```

Latency: ~1s over LAN. Acceptable even synchronous, but async is cleaner.

### 3. Card→machine mapping?
Two approaches, smallest first:

**Option A (now):** Convention-based. Cards with labels `chunk:app` that touch gallery/media features implicitly depend on Bedroom. The boot check output tells the role "don't pull media cards."

**Option B (later):** Explicit `requires:Bedroom` label on cards. `board-ts` can filter and flag. But this requires label discipline — adds friction.

Recommend Option A now, Option B when we have more than 2 machines.

## Smallest Viable Scope

1. Add `services.json` with Bedroom services, health checks, and feature mapping (~30 lines)
2. Add `bedroom-health.sh` check to `werk-init.sh` output (~20 lines)
3. Surface result in session-start file

That's small. I can do this in the current session if there's card space.
