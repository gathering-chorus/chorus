#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of qual-golden-api-regression; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src/handlers platform/api/tests/regression
printf 'export function zzHandler() { return {}; }\n' > platform/api/src/handlers/zz-handler.ts
printf '// names no handler\n' > platform/api/tests/regression/other.regression.test.ts
git add -A
