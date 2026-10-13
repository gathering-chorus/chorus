#!/usr/bin/env bash
# #4022 — "did any file change since the stamp?" for the index crawler's trigger.
# The old `-not -path "*/node_modules/*"` filtered the answer but find still
# walked every node_modules and target tree: 73.5s over platform/ per crawler
# run (2026-10-13). -prune stops find at the directory, so it is never opened.
# Source this file; do not execute. Prints the first newer file, or nothing.
changed_since() { # $1 stamp file, $2 dir
  find "$2" \( -name node_modules -o -name target -o -name dist -o -name .git \) -prune \
    -o -type f -newer "$1" -print -quit
}
