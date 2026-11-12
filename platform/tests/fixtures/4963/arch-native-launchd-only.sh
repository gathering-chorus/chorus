#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-native-launchd-only; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p svc
printf 'FROM scratch\n' > svc/Dockerfile
git add -A
