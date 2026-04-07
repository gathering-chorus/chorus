# /listen — Voice Input

This skill captures Jeff's voice — from a file, the Mac mic, or the andon menu bar button — and transcribes it so you can read what he said. It eliminates the typing bottleneck for creative, exploratory, or on-the-go communication.

## Two Input Paths

1. **Andon mic button** (preferred) — Jeff clicks the mic icon in the menu bar, speaks, clicks again to stop. Transcription is injected directly into the active role's session. No `/listen` command needed — it just arrives as text.

2. **`/listen` command** — explicit invocation for longer voice input or audio file transcription.

Both paths use the same local whisper.cpp pipeline. All transcription runs on Apple Silicon (Metal GPU). No audio leaves the machine.

## How to Use

### Path 1: Andon voice (automatic)

Voice input from the andon mic button arrives as a user message with timestamp markers like `[00:00:00.000 --> 00:00:04.440]`. Treat it as Jeff's typed message — respond normally. The transcription text follows the timestamp block.

No action needed from the role — just respond to what Jeff said.

### Path 2: `/listen` command

```bash
/Users/jeffbridwell/CascadeProjects/platform/scripts/listen.sh [audio-file]
```

**Modes:**
- With a file path (`.m4a`, `.mp3`, `.wav`, etc.) — transcribe that file
- No arguments — toggle mic recording (start if not recording, stop if recording)
- `start` / `stop` — explicit control

The script outputs the transcription text to stdout. Also saved to `/tmp/chorus-listen/latest.txt`.

### Responding to voice input

Treat the transcription as Jeff's message. He spoke instead of typed — respond normally. If the transcription seems garbled or unclear, ask Jeff to clarify rather than guessing.

## Privacy Note

All transcription runs locally via whisper.cpp on Apple Silicon (Metal GPU). No audio or text leaves the machine. Audio files are stored in `/tmp/chorus-listen/` (ephemeral). Never persist raw audio beyond `/tmp/chorus-listen/`.

## Examples

Jeff clicks andon mic, speaks "move card 1099 to done", clicks stop
-> Text arrives in session. Role executes the board command.

Jeff: `/listen`
-> Record from mic, transcribe, respond to what he said

Jeff: `/listen /path/to/voice-memo.m4a`
-> Transcribe the file, respond to what he said

## When to Use Proactively

If Jeff mentions a voice memo, audio file, or says "let me just say this" — suggest `/listen` so he can speak instead of type. But the andon button is faster for quick input.
