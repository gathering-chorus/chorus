#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of qual-warning-ratchet-downward; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/zz
printf '{"name":"zz"}\n' > platform/zz/package.json
git add -A
