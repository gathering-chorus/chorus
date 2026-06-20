#!/usr/bin/env bash
# #3519 — ci-main-red must NOT silently report "ok" when it cannot verify CI
# (GH_TOKEN absent). A blind check that says "ok" is a false-negative: main
# can be genuinely red and never surface. It must report the unverifiable state.
set -uo pipefail
YML="${1:-$(cd "$(dirname "$0")/../../domains/alerts" && pwd)/ci-main-red.yml}"
check=$(awk '/^check: \|/{f=1;next} /^[a-z_]+:/{if(f)exit} f{print}' "$YML")
out=$(GH_TOKEN="" bash -c "$check" 2>&1) || true
echo "check output (GH_TOKEN absent): '$out'"
if [ "$out" = "ok" ]; then
  echo "RED: ci-main-red reports 'ok' with no token — a real red main would never surface (false-negative)"
  exit 1
fi
echo "PASS: ci-main-red surfaces the unverifiable state instead of silently passing"
exit 0
