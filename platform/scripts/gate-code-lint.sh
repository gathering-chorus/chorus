#!/usr/bin/env bash
# gate-code-lint.sh — Block execSync in request-path code (#2000)
# Usage: gate-code-lint.sh <file1.ts> [file2.ts ...]
# Exit 0 = clean, Exit 1 = execSync found in request-path code
#
# Request-path dirs (BLOCKED): src/handlers/, src/services/, src/middleware/
# Allowed dirs: scripts/, tests/, build tooling, anything else

set -uo pipefail

BLOCKED_PATTERNS="src/handlers/|src/services/|src/middleware/"
VIOLATIONS=0

if [[ $# -eq 0 ]]; then
  echo "Usage: gate-code-lint.sh <file1.ts> [file2.ts ...]"
  exit 0
fi

for file in "$@"; do
  # Skip non-.ts files
  [[ "$file" != *.ts ]] && continue

  # Check if file is on a request path
  if echo "$file" | grep -qE "$BLOCKED_PATTERNS"; then
    # Search for execSync in the file
    if grep -n 'execSync' "$file" 2>/dev/null | grep -q .; then
      MATCHES=$(grep -n 'execSync' "$file" 2>/dev/null)
      echo "FAIL: execSync in request-path code: $(basename "$file")"
      while IFS= read -r line; do
        echo "  $line"
      done <<< "$MATCHES"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  fi
done

if [[ $VIOLATIONS -gt 0 ]]; then
  echo ""
  echo "execSync blocks the Node.js event loop. Use async alternatives:"
  echo "  execSync → exec (child_process) or spawn"
  echo "  execSync → worker_threads for CPU-bound work"
  exit 1
fi

exit 0
