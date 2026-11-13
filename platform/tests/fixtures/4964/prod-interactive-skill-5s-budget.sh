#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of prod-interactive-skill-5s-budget; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/skills/zz
printf -- '---\nname: zz\nuser-invocable: true\n---\n# zz\n\n```bash\nstep-one\nstep-two\n```\n' > platform/skills/zz/SKILL.md
git add -A
