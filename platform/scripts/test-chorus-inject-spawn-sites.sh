#!/usr/bin/env bash
# test-chorus-inject-spawn-sites.sh — #2820 grep-gate (replacement for
# `tests/nudge_single_drain.rs` retired in #2814).
#
# Invariant (#2283): one drain point. Pulse's DeliveryWorker is the only
# canonical caller of `chorus-inject`. Pre-#2804 this was protected by a
# Rust source-grep on `nudge.rs`; post-#2804 the canonical lives in
# `pulse/src/service.ts` (which builds runInject + selfTest and hands
# them to the worker). A future addition of a chorus-inject spawn site
# OUTSIDE that file silently re-introduces the duplicate-delivery class.
#
# Whitelist:
#   - platform/pulse/src/service.ts           (canonical: runInject + selfTest)
#   - **/tests/**, test-*, *.test.*, *.bats   (test surfaces — exercise the binary)
#   - platform/pulse/dist/                    (build artifacts of the canonical)
#   - .git/, target/, node_modules/, dist/    (vendored / build / vcs)
#
# Hits anywhere else fail with a message naming #2283.
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

# Spawn-shape patterns across languages.
PATTERNS=(
  'spawn[(][^)]*chorus-inject'                 # TS/JS spawn(..., 'chorus-inject', ...)
  'spawn[(][^)]*injectBin'                      # TS/JS spawn(injectBin, ...) — pulse's variable
  'Command::new[(][^)]*chorus-inject'           # Rust Command::new("…/chorus-inject")
  'Command::new[(]INJECT_BIN'                   # Rust test pattern (whitelisted via tests/)
  'exec[[:space:]]+[^[:space:]]*chorus-inject'  # shell exec chorus-inject
  '\bbash[[:space:]]+[^[:space:]]*chorus-inject' # shell bash chorus-inject
)

EXCLUDE_PATTERN='/\.git/|/target/|/node_modules/|/dist/|/tests/|test-|_test\.|\.test\.|\.bats$'

# Whitelisted canonical spawn site.
CANONICAL='platform/pulse/src/service.ts'

cd "$REPO_ROOT"

SEARCH_DIRS=(platform skills proving)

offenders=()
for pat in "${PATTERNS[@]}"; do
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    file="${line%%:*}"
    file="${file#./}"
    [ "$file" = "$CANONICAL" ] && continue
    echo "$file" | grep -qE "$EXCLUDE_PATTERN" && continue
    offenders+=("$file: $line")
  done < <(grep -rEn \
    --exclude-dir=node_modules --exclude-dir=target --exclude-dir=dist --exclude-dir=.git \
    "$pat" "${SEARCH_DIRS[@]}" 2>/dev/null)
done

if [ ${#offenders[@]} -gt 0 ]; then
  echo "FAIL (#2820, replaces #2283): chorus-inject spawn sites outside $CANONICAL:"
  printf '  %s\n' "${offenders[@]}"
  echo
  echo "  #2283 invariant: one drain point. Pulse's DeliveryWorker (via service.ts"
  echo "  runInject) is the only canonical caller of chorus-inject. Adding new"
  echo "  spawn sites silently re-introduces the duplicate-delivery class."
  echo "  If a new spawn site is intentional, update this test's whitelist with"
  echo "  the design rationale."
  exit 1
fi

echo "PASS: chorus-inject spawn site is single — only $CANONICAL"
exit 0
