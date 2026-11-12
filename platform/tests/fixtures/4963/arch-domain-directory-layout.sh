#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-domain-directory-layout; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p directing
echo 'a stray note at the stream root' > directing/notes.md
git add -A
