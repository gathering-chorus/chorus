#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-harness-adapter-boundary; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
echo 'execSync("osascript -e beep");' > platform/api/src/ding.ts
git add -A
