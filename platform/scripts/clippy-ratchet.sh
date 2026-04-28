#!/usr/bin/env bash
# #2532: per-lint cargo clippy ratchet. Thin wrapper that delegates to clippy-ratchet.py.
exec python3 "$(dirname "$0")/clippy-ratchet.py" "$@"
