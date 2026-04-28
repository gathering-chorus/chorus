#!/usr/bin/env bash
# Hermetic tests for doc-inventory-classify.py + doc-inventory-reconcile.py (#2514).
# Wraps two unittest modules so they run via the existing platform/tests/ entrypoint.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FAILED=0

for t in test_doc_inventory_classify.py test_doc_inventory_reconcile.py; do
  echo "--- $t ---"
  if ! python3 "$SCRIPT_DIR/$t"; then
    FAILED=1
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "FAIL: one or more python test modules failed"
  exit 1
fi
echo "OK: doc-inventory python tests"
