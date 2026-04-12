#!/usr/bin/env bash
# loki-tunnel.sh — resilient reverse tunnel to Bedroom for log shipping (#1917)
# Clears stale port holder before connecting. Called by LaunchAgent.
set -euo pipefail

REMOTE="192.168.86.242"
PORT="3102"
LOG_TAG="loki-tunnel"
MAX_RETRIES=3

# Clear stale sshd holding the port on Bedroom before connecting.
# Retry because the port can be re-grabbed between clear and connect.
for i in $(seq 1 $MAX_RETRIES); do
  stale_pid=$(ssh -o ConnectTimeout=5 "$REMOTE" "lsof -t -i :${PORT} -sTCP:LISTEN" 2>/dev/null || true)
  if [ -z "$stale_pid" ]; then
    break
  fi
  echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] clearing stale listener PID $stale_pid on $REMOTE (attempt $i)"
  ssh -o ConnectTimeout=5 "$REMOTE" "kill $stale_pid" 2>/dev/null || true
  sleep 2
done

echo "$(date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] connecting -R ${PORT}:localhost:${PORT} ${REMOTE}"

exec /usr/bin/ssh -N \
  -o ServerAliveInterval=60 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  -o ConnectTimeout=10 \
  -R "${PORT}:localhost:${PORT}" \
  "$REMOTE"
