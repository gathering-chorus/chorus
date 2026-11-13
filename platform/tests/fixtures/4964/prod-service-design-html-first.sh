#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of prod-service-design-html-first; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p designing/docs
printf '# zz service design\n' > designing/docs/zz-service-design.md
git add -A
