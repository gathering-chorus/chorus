#!/bin/bash
exec "$(dirname "$0")/session-end-hook" "$@"
