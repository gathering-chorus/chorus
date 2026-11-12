#!/bin/sh
# #4960 — a git tree in $1 holding one violation of policy-qual-no-compiled-artifacts-in-src; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p pkg/src
echo "x" > pkg/src/a.js
git add -A
