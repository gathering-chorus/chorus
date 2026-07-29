#!/usr/bin/env bash
# ci-main-red.check.sh — #3709: the ci-main-red check, EXTRACTED from its yml.
#
# Why a script and not an inline block: alert-runner.sh extracts an inline
# check with `awk '/^check: \|/{f=1;next} /^[a-z]/{if(f)exit} f{print}'`, which
# stops at the first column-0 lowercase line. Embedded python (`import ...`,
# `try:`) sits at column 0, so the runner silently cut this script in half and
# ran something that did not parse — the check could never report CI's state,
# only its own early-exit. Same move #2327 made for fuseki-harvest.
set -uo pipefail

# Query the GitHub Actions API for the most recent run of the quality
# workflow on the main branch. Requires GH_TOKEN env var (PAT with
# actions:read scope).
# #3709 — the slug was stale: the repo moved to gathering-chorus/chorus, so
# GitHub answered 301 and (curl -sf does not follow redirects) the body was a
# "Moved Permanently" JSON the parser could not read. Combined with the
# swallow-on-parse-error below, the check printed nothing and exited 0 — a
# silent pass over a main that had been red for 39 consecutive runs.
REPO="${CI_MAIN_RED_REPO:-gathering-chorus/chorus}"
WORKFLOW="${CI_MAIN_RED_WORKFLOW:-quality.yml}"

# #3709 — ASK BEFORE GIVING UP. alert-runner.sh sources no environment, so
# GH_TOKEN is never set under launchd and #3519's unverifiable branch fired
# unconditionally every night for months — while quality.yml on main sat red
# for 39 consecutive runs (Jun 21 → Jul 29) and nobody looked, because the
# alarm reported its own blindness rather than CI's state and that read as
# noise. `gh` is authenticated on this host and holds a token in its keyring;
# a check that never asks it is not unverifiable, it is unasked.
if [ -z "${GH_TOKEN:-}" ]; then
  GH_TOKEN="$(gh auth token 2>/dev/null || true)"
fi

if [ -z "${GH_TOKEN:-}" ]; then
  # #3519 — Genuinely no token = CI state is UNVERIFIABLE, not ok. Silently
  # returning ok is a false-negative: a genuinely red main never surfaces.
  # Report the observation (can't see CI) instead of guessing it's fine.
  echo "unverifiable:GH_TOKEN-absent"
  exit 1
fi

RESULT=$(curl -sfL --max-time 10 \
  -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/${REPO}/actions/workflows/${WORKFLOW}/runs?branch=main&per_page=1" 2>/dev/null)

STATUS=$(echo "$RESULT" | python3 -c "
import sys, json
try:
  d = json.load(sys.stdin)
  # #3709 — 'the key is absent' and 'the list is empty' are DIFFERENT states and
  # were conflated. A 301/error body has no workflow_runs key at all; reporting
  # that as no-runs mapped it to exit 0, so a wrong-shaped answer read as green.
  # Genuinely-empty stays no-runs (a workflow that has never run is not a red).
  if 'workflow_runs' not in d:
      print('unreadable:no-workflow_runs-key')
      raise SystemExit(0)
  runs = d['workflow_runs']
  if not runs:
      print('no-runs')
  else:
      r = runs[0]
      s = r.get('status', '')
      c = r.get('conclusion', '')
      # status=completed + conclusion=failure → red
      # status=in_progress / queued → not yet a verdict, treat as ok
      if s == 'completed' and c == 'failure':
          print(f'red:{r.get(\"html_url\", \"\")}')
      else:
          print('ok')
except Exception as e:
  print(f'parse-error:{e}')
" 2>/dev/null)

case "$STATUS" in
  ok|no-runs) exit 0 ;;
  red:*) echo "$STATUS"; exit 1 ;;
  # #3709 — an unreadable answer is UNVERIFIABLE, not ok. This branch exited 0
  # on a parse error "to avoid a false fire", which is the same false-negative
  # #3519 removed from the token path: a check that cannot read CI must say so,
  # never imply green. The 301 above fell into exactly this branch.
  *) echo "unverifiable:${STATUS:-empty-response}"; exit 1 ;;
esac

