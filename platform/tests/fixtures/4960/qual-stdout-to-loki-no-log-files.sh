#!/bin/sh
# @domain: policies
# #4960 — a git tree in $1 holding one violation of policy-qual-stdout-to-loki-no-log-files; the check run there must count it.
set -e
cd "$1"
git init -q .
echo "x" > run.log
git add -A
