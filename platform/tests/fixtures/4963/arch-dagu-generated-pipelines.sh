#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-dagu-generated-pipelines; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/pipelines
printf 'steps:\n  - name: by-hand\n' > platform/pipelines/by-hand.yaml
git add -A
