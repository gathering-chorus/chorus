# Spike: Voice-to-Session (#1099)

**From:** Silas
**Date:** 2026-03-07
**Type:** spike findings
**Cards:** #1099, #547 (/listen receive mode), #269 (/talk floating voice)

## Findings

### What exists today

`/listen` skill + `chorus/scripts/listen.sh` — captures mic via sox, transcribes via whisper-cli (local, Apple Silicon Metal GPU), outputs text to the active session. Works within a single session only. No cross-session routing.

### Research results

**1. Role-to-TTY mapping (solved)**

Three Claude processes map deterministically to roles via CWD:

| TTY | PID | CWD | Role |
|-----|-----|-----|------|
| ttys000 | 36083 | product-manager/ | Wren |
| ttys001 | 30129 | architect/ | Silas |
| ttys002 | 47929 | engineer/ | Kade |

Discovery: `ps -eo pid,tty,comm | grep claude` + `lsof -p <pid> -Fn` gives the mapping. Terminal.app exposes TTY per tab via AppleScript (`tty of tab`).

**2. Transcription latency**

| Model | Size | 2s audio | 5s budget fit? |
|-------|------|----------|---------------|
| ggml-medium.en | 1.4GB | 4.2s | No (no headroom) |
| ggml-small.en | 465MB | 1.7s | Yes (3.3s for capture + route + inject) |

Recommendation: **small.en for voice-to-session** (speed > quality for directed speech). Keep medium.en for longer-form transcription (/listen receive mode, #547).

**3. Session injection — three approaches**

| Approach | Mechanism | Latency | Focus steal? | Complexity |
|----------|-----------|---------|-------------|------------|
| **A. Terminal injection** | osascript types into target tab via TTY lookup | <1s after transcription | Yes — activates target window | Medium |
| **B. Clipboard + notify** | Paste to clipboard, notify Jeff which tab | <1s | No | Low |
| **C. Voice inbox** | Write to `/tmp/voice-inbox/<role>/`, hook picks up on next turn | Next response cycle | No | Low |

**Approach A is the only one that meets the AC** ("Jeff speaks, transcript appears in target role's session"). The focus-steal concern is real but manageable — Jeff is directing a specific role, so switching to that tab is arguably the right behavior.

Injection mechanism for Approach A:
```
1. Copy transcript to clipboard (pbcopy)
2. osascript: find Terminal tab where tty = target role's TTY
3. Activate that window
4. Paste (keystroke "v" using command down)
5. Press Enter (keystroke return)
```

Safety gate: check `ps -o stat= -p <pid>` — only inject if process state is `S+` (idle, waiting for input). If role is processing, queue to voice inbox (Approach C fallback).

**4. Routing**

Simplest v1: explicit prefix detection in transcript.
- "hey kade" / "kade" at start → route to Kade
- "hey silas" / "silas" at start → route to Silas
- "hey wren" / "wren" at start → route to Wren
- No prefix → route to last active role (from `role-state.sh`)

v2 (future): keyword inference from card context.

**5. Claude Code programmatic input**

`--input-format stream-json` exists but only works with `--print` (non-interactive headless mode). Not usable for injecting into interactive sessions. Terminal injection (Approach A) is the viable path.

## Card consolidation

#1099, #547, #269 share the same capture/transcribe infrastructure. Differences:

| Card | Focus | Mode | Routing |
|------|-------|------|---------|
| #1099 | Direct commands to roles | Push-to-talk | Cross-session |
| #547 | Stream of consciousness | Continuous/long-form | Current session |
| #269 | Hands-free from anywhere | Always-on, floating | Cross-session |

Recommendation: #1099 is the **infrastructure card** — build the pipeline (capture, transcribe, route, inject). #547 and #269 are **UX modes** that sit on top of it. Ship #1099 first, then #547/#269 become small.

## Recommended architecture

```
[mic] → [sox capture] → [whisper-cli small.en] → [route by prefix]
                                                         ↓
                                          [osascript inject to target TTY]
                                                         ↓
                                              [spine event emitted]
```

Single script: `voice-to-session.sh [start|stop]`
- Toggle: start/stop mic recording (same pattern as listen.sh)
- On stop: transcribe → route → inject → emit event
- Fallback: if target role is busy, queue to voice inbox

## Latency budget

| Step | Time |
|------|------|
| Sox stop + finalize | ~0.5s |
| Whisper small.en (2-5s speech) | ~1.7-3s |
| Route detection | <0.1s |
| osascript inject | <0.5s |
| **Total** | **~2.8-4.1s** |

Within the 5s budget (DEC-071).

## Next steps

1. Build `voice-to-session.sh` — capture, transcribe (small.en), route, inject
2. Test with real speech (not sine wave benchmark)
3. Wire as LaunchAgent or keep as manual invocation?
4. Brief Wren: #547/#269 can build on this infrastructure

## Open questions for Jeff

- Push-to-talk (say `/listen`, speak, say `/listen` again) vs hardware trigger (keyboard shortcut)?
- OK with focus stealing when routing to a different role's tab?
- Start with push-to-talk v1, add always-on v2 later?
