#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-language-boundary; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/services/zz-daemon/src
printf 'while true; do date; done\n' > platform/services/zz-daemon/src/loop.sh
git add -A
