#!/usr/bin/env bash
# ROW 1 — the public root serves THE SITE, not a login form. (#3765)
#
# WHY THIS ROW EXISTS
#
# On 2026-08-08 the public root answered HTTP 200 with a login form. Three of us
# were looking at monitoring that checks status codes, and all three saw green.
# A 200 is not the assertion — the BODY is. "The server answered" and "a visitor
# got the site" are different facts, and every probe we owned could only see the
# first.
#
# THE TWO STATES THIS MUST SEPARATE
#   serving the site      → real navigation, substantial body, no credential form
#   serving a login wall  → a password field where the site should be
# Both are 200. A status check cannot tell them apart. That is the whole row.
#
# HOW IT IS PROVEN TO WORK
#   --fixture <file>  score a captured body instead of the live URL, so the red
#                     state is reproducible without breaking production
#   --expect-red      assert the scored body FAILS — the negative proof. A
#                     measure that has never been shown to go red is decoration.
set -uo pipefail

URL="${FITNESS_PUBLIC_ROOT:-https://lightlifeurbangardens.com/}"
FIXTURE=""
EXPECT_RED=0
while [ $# -gt 0 ]; do
  case "$1" in
    --fixture) FIXTURE="$2"; shift 2 ;;
    --expect-red) EXPECT_RED=1; shift ;;
    *) shift ;;
  esac
done

if [ -n "$FIXTURE" ]; then
  [ -f "$FIXTURE" ] || { echo "row1: fixture not found: $FIXTURE" >&2; exit 2; }
  body="$(cat "$FIXTURE")"
  src="fixture:$(basename "$FIXTURE")"
else
  body="$(curl -s --max-time 25 "$URL" 2>/dev/null)"
  src="$URL"
  # UNMEASURABLE is its own verdict, distinct from RED. A probe that reports
  # failure when it could not ask has told you nothing, and worse, it teaches
  # people to discount it.
  if [ -z "$body" ]; then
    echo "row1 UNMEASURABLE — no response from $src. Could not ask; this is not a finding about the site." >&2
    exit 2
  fi
fi

fail=""

# A credential FORM, not the word "login". A nav link reading "Sign in" is normal
# on a public site; an input taking a password where the site should be is the
# failure. Matching the word alone would cry wolf forever, and a probe that cries
# wolf gets ignored — which is worse than not having one.
if printf '%s' "$body" | grep -qiE '<input[^>]+type=["'"'"']?password'; then
  fail="$fail a password field is served where the site should be;"
fi

# The site's own navigation. If these are gone, the root is not the site whatever
# else it is. Deliberately paths that have existed for months.
# -o | wc -l, NOT grep -c: grep -c counts matching LINES. A page with all three
# links on one line scored 1 and read as red. It scored 3 on the live site only
# because that markup happens to break the lines — an accident, not a measure.
navcount=$(printf '%s' "$body" | grep -oE 'href="/(about|blog|chorus)"' | sort -u | wc -l | tr -d ' ')
if [ "${navcount:-0}" -lt 2 ]; then
  fail="$fail site navigation absent (found ${navcount:-0} of /about /blog /chorus);"
fi

# Substance. A login wall is small; the site is not. A floor, not a range —
# asserting a size window would go red on every content edit and teach us to
# ignore it.
bytes=$(printf '%s' "$body" | wc -c | tr -d ' ')
if [ "$bytes" -lt 4000 ]; then
  fail="$fail body is ${bytes} bytes, too small to be the site;"
fi

if [ -n "$fail" ]; then
  echo "row1 RED — $src is not serving the site:$fail"
  [ "$EXPECT_RED" -eq 1 ] && { echo "NEGATIVE PROOF OK — the probe goes red on a login wall."; exit 0; }
  exit 1
fi

if [ "$EXPECT_RED" -eq 1 ]; then
  echo "NEGATIVE PROOF FAILED — the probe passed a body it was built to reject ($src)." >&2
  echo "A measure that cannot go red on a failure we have actually lived is decoration." >&2
  exit 1
fi

echo "row1 GREEN — $src serves the site (${bytes} bytes, ${navcount} nav links, no credential form)"
