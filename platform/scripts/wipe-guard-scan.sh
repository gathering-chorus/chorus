#!/usr/bin/env bash
# wipe-guard-scan.sh — #3601: the ontology wipe guard.
#
# "There is no test env, only production." athena-deploy defaults ONTOLOGY_GRAPH
# to LIVE urn:chorus:ontology (athena-deploy/src/lib.rs:70), so a test that
# invokes a deploy surface without a test-graph override IS a wipe of the 34
# live-only domains. Today every test isolates by convention (07-01 audit);
# this scanner makes the convention structural.
#
# A test file is a VIOLATION when it:
#   (a) EXECUTES a deploy surface (chorus-model-deploy / athena-deploy — direct
#       or via a $SCRIPT/$DEPLOY variable) with no ONTOLOGY_GRAPH override
#       anywhere in the file, OR
#   (b) sets ONTOLOGY_GRAPH to the live graph explicitly, OR
#   (c) has a line issuing a graph-mutating update that names the live graph.
# Deliberately NOT violations (each burned a false positive in the first cut):
#   - reader tests that grep/sed/cat the deploy script's text
#   - test graphs that share the live prefix (urn:chorus:ontology-test-…)
#   - in-memory stores loading/deleting under the live graph NAME (no Fuseki)
# Escape hatch: a `wipe-guard: exempt — <reason>` comment (reason REQUIRED —
# a bare marker does not count; silence is how conventions rot).
#
# The guard's own files and fixtures/ dirs are excluded — they must talk ABOUT
# these patterns to test them (the #3725 self-match lesson).
#
# Usage: wipe-guard-scan.sh [<root>]   exits 0 clean / 1 with violations listed.

set -uo pipefail
ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
# Live graph with a boundary: never match urn:chorus:ontology-test-… prefixes.
LIVE_B='urn:chorus:ontology([^-A-Za-z0-9]|$)'
DEPLOY_RE='chorus-model-deploy|athena-deploy'
EXEC_RE='(^|[;&|([:space:]])(env[[:space:]][^|;&]*)?(bash|sh)[[:space:]]+[^#]*(chorus-model-deploy|athena-deploy|\$\{?(SCRIPT|DEPLOY))'
READER_RE='grep |sed |cat |head |awk |\[ -f '
MUTATE_RE='CLEAR GRAPH|DROP GRAPH|INSERT DATA|DELETE DATA|DELETE WHERE|/update'

violations=0

check_file() {
  local f="$1"
  case "$f" in
    *wipe-guard-scan.sh|*test-ontology-wipe-guard.sh|*/fixtures/*) return ;;
  esac
  if grep -qE 'wipe-guard: exempt (—|-|--) .+' "$f" 2>/dev/null; then return; fi
  local rel="${f#"$ROOT"/}"
  # (b) explicit live-graph override — always a violation.
  if grep -qE "ONTOLOGY_GRAPH=[\"']?${LIVE_B}" "$f" 2>/dev/null; then
    echo "WIPE-VECTOR (live-graph override): $rel"
    violations=$((violations+1))
    return
  fi
  # (c) a single line that both mutates and names the live graph.
  if grep -E "$MUTATE_RE" "$f" 2>/dev/null | grep -qE "$LIVE_B"; then
    echo "WIPE-VECTOR (mutating update names live graph): $rel"
    violations=$((violations+1))
    return
  fi
  # (a) executed deploy surface with no override anywhere in the file.
  if grep -qE "$DEPLOY_RE" "$f" 2>/dev/null \
    && grep -E "$EXEC_RE" "$f" 2>/dev/null | grep -vE "$READER_RE" | grep -q . \
    && ! grep -q 'ONTOLOGY_GRAPH' "$f" 2>/dev/null; then
    echo "WIPE-VECTOR (deploy surface executed, no ONTOLOGY_GRAPH override): $rel"
    violations=$((violations+1))
  fi
}

# git ls-files is ~100x faster than find and pre-commit runs this on every
# commit; the find branch covers non-repo roots (the guard test's fixtures).
list_files() {
  if git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$ROOT" ls-files -z -- \
      'platform/tests/*.bats' 'platform/scripts/test-*.sh' \
      '*.test.ts' 'platform/services/*/tests/*.rs' \
      | tr '\0' '\n' | sed "s|^|$ROOT/|"
  else
    find "$ROOT/platform/tests" -name '*.bats' -type f 2>/dev/null
    find "$ROOT/platform/scripts" -maxdepth 1 -name 'test-*.sh' -type f 2>/dev/null
    find "$ROOT" \( -path '*/node_modules' -o -path '*/target' -o -path '*/.git' -o -path '*/coverage' -o -path '*/dist' \) -prune \
      -o -name '*.test.ts' -type f -print 2>/dev/null
    find "$ROOT/platform/services" -path '*/tests/*.rs' -not -path '*/target/*' -type f 2>/dev/null
  fi
}

# One xargs-grep pre-pass names the handful of candidate files; only those pay
# the per-file rule checks (the naive per-file loop cost ~10s of grep spawns).
while IFS= read -r f; do
  [ -f "$f" ] && check_file "$f"
done < <(list_files | tr '\n' '\0' \
  | xargs -0 grep -lE 'chorus-model-deploy|athena-deploy|urn:chorus:ontology|ONTOLOGY_GRAPH' 2>/dev/null)

if [ "$violations" -gt 0 ]; then
  echo ""
  echo "wipe-guard: $violations test file(s) can reach the LIVE ontology graph (#3601)."
  echo "Isolate with ONTOLOGY_GRAPH=<test graph>, or add: wipe-guard: exempt — <why it is safe>"
  exit 1
fi
echo "wipe-guard: clean — no test can reach the live ontology graph"
exit 0
