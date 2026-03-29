#!/bin/bash
# site-walkthrough.sh — Headless Chrome walkthrough (desktop + mobile)
# Usage: bash scripts/site-walkthrough.sh [--mobile] [--desktop] [--both] [output-dir]
# Delegates to site-walkthrough.mjs (Puppeteer, headless, zero focus-stealing)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Ensure puppeteer is available
if ! command -v npx >/dev/null 2>&1; then
  echo "ERROR: npx not found — install Node.js" >&2
  exit 1
fi

# Pass all args through to the Node script
exec node "$SCRIPT_DIR/site-walkthrough.mjs" "$@"
