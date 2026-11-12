#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-cmdb-service-registered; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p config/launchagents
printf '<plist><dict><key>Label</key><string>com.chorus.zz-unregistered</string></dict></plist>\n' > config/launchagents/com.chorus.zz-unregistered.plist
git add -A
