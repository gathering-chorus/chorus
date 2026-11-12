#!/bin/sh
# #4960 — a git tree in $1 holding one violation of policy-arch-localhost-internal-binds; the check run there must count it.
set -e
cd "$1"
git init -q .
echo "<string>0.0.0.0</string>" > com.x.plist
git add -A
