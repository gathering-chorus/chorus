#!/usr/bin/env bash
# loki-tunnel.sh — resilient reverse tunnel to Bedroom for log shipping (#1917)
# Clears stale port holder before connecting. Called by LaunchAgent.
set -euo pipefail

REMOTE="192.168.86.242"
PORT="3102"
LOG_TAG="loki-tunnel"

# Clear stale sshd holding the port on Bedroom before connecting.
# This is the root cause of tunnel flapping — previous connection dies,
# remote sshd holds the port, new connection can't bind.
stale_pid=$(ssh -o ConnectTimeout=5 "$REMOTE" "lsof -t -i :${PORT} -sTCP:LISTEN" 2>/dev/null || true)
if [ -n "$stale_pid" ]; then
  echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] clearing stale listener PID $stale_pid on $REMOTE"
  ssh -o ConnectTimeout=5 "$REMOTE" "kill $stale_pid" 2>/dev/null || true
  sleep 2
fi

echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] connecting -R ${PORT}:localhost:${PORT} ${REMOTE}"

exec /usr/bin/ssh -N \
  -o ServerAliveInterval=60 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  -o ConnectTimeout=10 \
  -R "${PORT}:localhost:${PORT}" \
  "$REMOTE"
