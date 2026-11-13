#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of qual-negative-proof-for-gates; the check run there must count it.
set -e
cd "$1"
git init -q .
printf '{"data":[{"name":"zz-gate","checkKind":"command","checkSpec":"true","baseline":"0","status":"ratchet","violationFixture":"platform/tests/fixtures/zz-missing.sh"}]}\n' > zz-policies.json
git add -A
