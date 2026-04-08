#!/bin/bash
# listen.sh — Voice input for Chorus sessions
# Usage: listen.sh [audio-file]    Transcribe an existing audio file
#        listen.sh start            Start recording from mic (background)
#        listen.sh stop             Stop recording and transcribe
#        listen.sh                  Same as "start" if not recording, "stop" if recording
#
# Outputs transcription text to stdout. Errors to stderr.

set -euo pipefail

MODELS_DIR="${CHORUS_WHISPER_MODELS:-$HOME/models}"
MODEL="${CHORUS_WHISPER_MODEL:-ggml-small.en.bin}"
LISTEN_DIR="/tmp/chorus-listen"
PID_FILE="$LISTEN_DIR/recording.pid"
WAV_FILE="$LISTEN_DIR/recording.wav"
TIMESTAMP=$(date +%Y%m%dT%H%M%S)
CHORUS_LOG="$(dirname "$0")/../../../scripts/chorus-log"

mkdir -p "$LISTEN_DIR"

# Ensure homebrew tools are on PATH (matches voice-to-session.sh)
export PATH="/opt/homebrew/bin:$PATH"

# Set Metal acceleration for whisper.cpp (Apple Silicon GPU)
WHISPER_PREFIX=$(brew --prefix whisper-cpp 2>/dev/null || true)
if [ -n "$WHISPER_PREFIX" ] && [ -d "$WHISPER_PREFIX/share/whisper-cpp" ]; then
    export GGML_METAL_PATH_RESOURCES="$WHISPER_PREFIX/share/whisper-cpp"
fi

# Check dependencies
for cmd in whisper-cli ffmpeg sox; do
    command -v "$cmd" >/dev/null 2>&1 || {
        echo "ERROR: $cmd not installed. Run: brew install $cmd" >&2
        exit 1
    }
done

# Check model exists
check_model() {
    if [ ! -f "$MODELS_DIR/$MODEL" ]; then
        echo "ERROR: Model not found at $MODELS_DIR/$MODEL" >&2
        exit 1
    fi
}

# Transcribe a WAV file and output text
transcribe() {
    local wav="$1"
    check_model
    whisper-cli -m "$MODELS_DIR/$MODEL" -f "$wav" -otxt -of "$LISTEN_DIR/transcript" 2>/dev/null
    cat "$LISTEN_DIR/transcript.txt"
    ln -sf "$LISTEN_DIR/transcript.txt" "$LISTEN_DIR/latest.txt"
}

# Check if recording is active
is_recording() {
    [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

# Start recording in background
start_recording() {
    if is_recording; then
        echo "Already recording (PID $(cat "$PID_FILE")). Run: listen.sh stop" >&2
        exit 1
    fi

    # Remove stale files
    rm -f "$WAV_FILE" "$PID_FILE"

    # Audio feedback: play start chime
    afplay /System/Library/Sounds/Tink.aiff &

    # Set terminal tab title to show recording state
    osascript -e 'tell application "Terminal" to set custom title of selected tab of front window to "🔴 LISTENING..."' 2>/dev/null || true

    # Start sox recording in background
    sox -d -r 16000 -c 1 -b 16 "$WAV_FILE" 2>/dev/null &
    local pid=$!
    echo "$pid" > "$PID_FILE"
    echo "🔴 LISTENING — speak now. Run /listen again to stop."
}

# Stop recording and transcribe
stop_recording() {
    if ! is_recording; then
        echo "ERROR: No active recording" >&2
        exit 1
    fi

    local pid
    pid=$(cat "$PID_FILE")

    # Send SIGTERM to sox — it finalizes the WAV file gracefully
    kill "$pid" 2>/dev/null || true
    # Wait for sox to finish writing
    sleep 0.5

    rm -f "$PID_FILE"

    # Audio feedback: play stop chime
    afplay /System/Library/Sounds/Pop.aiff &

    # Restore terminal tab title
    osascript -e 'tell application "Terminal" to set custom title of selected tab of front window to ""' 2>/dev/null || true

    if [ ! -f "$WAV_FILE" ] || [ ! -s "$WAV_FILE" ]; then
        echo "ERROR: No audio captured" >&2
        exit 1
    fi

    DURATION=$(sox "$WAV_FILE" -n stat 2>&1 | grep "Length" | awk '{print $3}' || echo "unknown")
    echo "Recorded ${DURATION}s. Transcribing..." >&2

    local START_NS=$(date +%s%N)
    transcribe "$WAV_FILE"
    local END_NS=$(date +%s%N)
    local LATENCY_MS=$(( (END_NS - START_NS) / 1000000 ))
    local CHARS=$(wc -c < "$LISTEN_DIR/transcript.txt" | tr -d ' ')

    # Emit spine event
    if [ -x "$CHORUS_LOG" ]; then
        "$CHORUS_LOG" "voice.received" "jeff" "source=listen,latency_ms=$LATENCY_MS,chars=$CHARS,duration=${DURATION}s" 2>/dev/null || true
    fi

    # Archive the recording
    mv "$WAV_FILE" "$LISTEN_DIR/mic-$TIMESTAMP.wav"
}

# --- Main ---

ARG="${1:-}"

# File mode: transcribe an existing audio file
if [ -n "$ARG" ] && [ -f "$ARG" ]; then
    WAV="$LISTEN_DIR/input-$TIMESTAMP.wav"
    echo "Converting..." >&2
    ffmpeg -y -i "$ARG" -ar 16000 -ac 1 -c:a pcm_s16le "$WAV" 2>/dev/null
    echo "Transcribing..." >&2
    START_NS=$(date +%s%N)
    transcribe "$WAV"
    END_NS=$(date +%s%N)
    LATENCY_MS=$(( (END_NS - START_NS) / 1000000 ))
    CHARS=$(wc -c < "$LISTEN_DIR/transcript.txt" | tr -d ' ')
    if [ -x "$CHORUS_LOG" ]; then
        "$CHORUS_LOG" "voice.received" "jeff" "source=listen-file,latency_ms=$LATENCY_MS,chars=$CHARS" 2>/dev/null || true
    fi
    exit 0
fi

# Explicit start/stop
case "$ARG" in
    start)
        start_recording
        ;;
    stop)
        stop_recording
        ;;
    "")
        # Toggle: if recording, stop. If not, start.
        if is_recording; then
            stop_recording
        else
            start_recording
        fi
        ;;
    *)
        echo "Usage: listen.sh [audio-file | start | stop]" >&2
        exit 1
        ;;
esac
