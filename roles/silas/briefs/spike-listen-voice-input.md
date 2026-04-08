# Spike: /listen — Voice Input for Chorus Sessions

**Author**: Silas (Architect)
**Date**: 2026-02-21
**Card**: C#32
**Time-box**: One session

## The Idea

Jeff talks instead of types. Voice memo or live mic → transcription → piped into the conversation as text. Preserves raw thinking without the reflection-then-articulate bottleneck that typing imposes. Especially valuable for creative/exploratory work, walks, garden time.

## Two Modes

### Mode 1: Voice Memo (file-based)
Jeff records a voice memo (phone, Mac, wherever) → `/listen /path/to/audio.m4a` → transcription → injected as context.

### Mode 2: Live Mic (real-time)
Jeff says `/listen` → mic captures until he stops talking → transcription → injected as his message.

## Tool Landscape (All Local, No Cloud)

Everything runs on M1 16GB with no data leaving the machine.

### whisper.cpp (Recommended — Primary)

```bash
brew install whisper-cpp ffmpeg sox
```

| Model | Disk | RAM | Accuracy | M1 Speed |
|-------|------|-----|----------|----------|
| small.en | 466 MB | ~852 MB | Good | ~6x real-time |
| medium.en | 1.5 GB | ~2.1 GB | Very good | ~2x real-time |

**File transcription:**
```bash
ffmpeg -i input.m4a -ar 16000 -ac 1 -c:a pcm_s16le /tmp/whisper-input.wav
whisper-cpp -m ~/models/ggml-medium.en.bin -f /tmp/whisper-input.wav -otxt
```

**Real-time mic (built-in):**
```bash
whisper-cpp-stream -m ~/models/ggml-small.en.bin --step 500 --length 5000
```

### Other Options Assessed

| Tool | Strengths | Weaknesses | Verdict |
|------|-----------|------------|---------|
| whisper.cpp | Both modes, Homebrew, Metal GPU, mature | — | **Use this** |
| lightning-whisper-mlx | 10x faster for files | Python-only, no real-time | Consider for batch |
| WhisperKit | Neural Engine, Swift-native | Newer, less battle-tested | Watch |
| Apple `hear` CLI | Zero model download | Lower accuracy, truncation issues | Skip |
| MLX Whisper | Good accuracy | File-only, Python | Skip (lightning is faster) |

### Audio Capture

```bash
# sox — simple mic recording (Ctrl-C to stop)
sox -d -r 16000 -c 1 -b 16 /tmp/recording.wav

# sox — timed recording (30 seconds)
sox -d -r 16000 -c 1 -b 16 /tmp/recording.wav trim 0 30

# ffmpeg — from Mac mic (device :0)
ffmpeg -f avfoundation -i ":0" -ar 16000 -ac 1 /tmp/recording.wav
```

## Architecture

### /listen (file mode)
```
Jeff: /listen /path/to/voice-memo.m4a
    ↓ chorus/scripts/listen.sh
ffmpeg converts to 16kHz WAV
    ↓
whisper-cpp transcribes (medium.en model)
    ↓
Transcription text returned to Claude Code session
    ↓
Claude reads it as Jeff's input
```

### /listen (live mode)
```
Jeff: /listen
    ↓ chorus/scripts/listen.sh
sox records from mic → /tmp/chorus-listen/recording.wav
    ↓ Jeff presses Ctrl-C or silence detection
whisper-cpp transcribes
    ↓
Transcription text returned to session
```

## Script Design: `chorus/scripts/listen.sh`

```bash
#!/bin/bash
# Usage: listen.sh [audio-file | mic]
# - With file path: transcribe existing audio
# - With "mic" or no args: record from mic, then transcribe

MODELS_DIR="${CHORUS_WHISPER_MODELS:-$HOME/models}"
MODEL="${CHORUS_WHISPER_MODEL:-ggml-medium.en.bin}"
LISTEN_DIR="/tmp/chorus-listen"
mkdir -p "$LISTEN_DIR"

if [ -n "$1" ] && [ "$1" != "mic" ] && [ -f "$1" ]; then
    # File mode: convert and transcribe
    INPUT="$1"
    WAV="$LISTEN_DIR/input-$(date +%Y%m%dT%H%M%S).wav"
    ffmpeg -y -i "$INPUT" -ar 16000 -ac 1 -c:a pcm_s16le "$WAV" 2>/dev/null
    whisper-cpp -m "$MODELS_DIR/$MODEL" -f "$WAV" -otxt -of "$LISTEN_DIR/transcript" 2>/dev/null
    cat "$LISTEN_DIR/transcript.txt"
else
    # Mic mode: record then transcribe
    WAV="$LISTEN_DIR/mic-$(date +%Y%m%dT%H%M%S).wav"
    echo "Listening... (press Ctrl-C to stop)" >&2
    sox -d -r 16000 -c 1 -b 16 "$WAV" 2>/dev/null
    echo "Transcribing..." >&2
    whisper-cpp -m "$MODELS_DIR/$MODEL" -f "$WAV" -otxt -of "$LISTEN_DIR/transcript" 2>/dev/null
    cat "$LISTEN_DIR/transcript.txt"
fi
```

## Skill Registration: `~/.claude/skills/listen/SKILL.md`

When Jeff says `/listen`:
1. If he provides a file path → transcribe it, show the text
2. If no path → record from mic, transcribe, show the text
3. Either way, the transcription becomes context for the conversation

The skill should tell Claude to read the transcription and respond to it as if Jeff typed it.

## Dependencies to Install

```bash
brew install whisper-cpp ffmpeg sox

# Download models
mkdir -p ~/models
curl -L -o ~/models/ggml-small.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin
curl -L -o ~/models/ggml-medium.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin

# Set Metal acceleration
echo 'export GGML_METAL_PATH_RESOURCES="$(brew --prefix whisper-cpp)/share/whisper-cpp"' >> ~/.zshrc
```

## Integration with The Clearing

Future: `/listen` in a Clearing session means Jeff speaks instead of types. The listen script captures → transcribes → Socket.IO emits it as Jeff's message. This is the path to voice-mode Clearing sessions.

## Privacy

- All transcription runs locally via whisper.cpp (Metal GPU on M1)
- No audio or text leaves the machine
- Audio files stored in `/tmp/chorus-listen/` (ephemeral)
- Fits the inner ring of the concentric trust model

## Effort Estimate

| Component | Lines | Risk |
|-----------|-------|------|
| listen.sh script | ~40 | Low |
| SKILL.md registration | ~30 | Low |
| Permission settings | ~3 | Low |
| Dependency install | 0 (brew commands) | Low — but model download is ~2GB |

**Total: ~75 lines. Half a session build (after deps install).**

## Open Questions

1. **Silence detection**: sox can detect silence to auto-stop recording (`silence 1 0.5 1%`). Worth adding for hands-free operation?
2. **Streaming mode**: whisper-cpp-stream does real-time continuous transcription. More complex but enables "Jeff talks, text appears live." Worth it for v1 or v2?
3. **Phone voice memos**: Jeff's capture pattern is often phone-based. How do voice memos get from iPhone to Mac? AirDrop? iCloud? The file needs to land somewhere accessible.
4. **Model choice**: medium.en (1.5GB, very good accuracy) vs small.en (466MB, good accuracy, 3x faster). Default to medium, let Jeff override?

## Recommendation

**Install deps now, build in the next session.** The script is ~40 lines wrapping whisper.cpp. The harder part is the model download (~2GB) and validating transcription quality on Jeff's voice. Start with file mode (voice memos), add mic mode once the pipeline proves out.
