#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-single-binary-deploy; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p config/launchagents
printf '<plist><dict><key>Label</key><string>com.chorus.zz</string><key>Program</key><string>/x/target/release/zz</string></dict></plist>\n' > config/launchagents/com.chorus.zz.plist
git add -A
