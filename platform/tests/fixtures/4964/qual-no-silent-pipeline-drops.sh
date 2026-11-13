#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of qual-no-silent-pipeline-drops; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
printf 'try { harvest(); } catch {}\n' > platform/api/src/zz-harvest.ts
git add -A
