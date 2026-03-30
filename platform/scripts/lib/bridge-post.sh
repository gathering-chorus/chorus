#!/bin/bash
# bridge-post.sh — shared Bridge POST helper with retry + logging
# Source this file, then call: bridge_post <url> <from> <message>
# Retries once on failure. Logs both attempts to stderr.

bridge_post() {
  local url="$1" from="$2" text="$3"
  local payload
  payload=$(jq -n --arg text "$text" --arg from "$from" '{from: $from, text: $text}')

  # First attempt
  if curl -sf --max-time 5 -X POST "$url" \
    -H 'Content-Type: application/json' \
    -d "$payload" &>/dev/null; then
    return 0
  fi

  echo "WARN: Bridge POST failed, retry in 2s" >&2

  # Retry once after short delay
  sleep 2
  if curl -sf --max-time 5 -X POST "$url" \
    -H 'Content-Type: application/json' \
    -d "$payload" &>/dev/null; then
    echo "INFO: Bridge POST retry succeeded" >&2
    return 0
  fi

  echo "ERROR: Bridge POST failed after retry — $url" >&2
  return 1
}
