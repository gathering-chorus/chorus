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
set -u
# Deliberately NOT pipefail. `printf ... | grep -q` returns grep's status, but
# grep -q exits the instant it matches, breaking the pipe under printf — and
# pipefail then reports the whole pipeline as failed, so a MATCH reads as a
# MISS. It surfaced in row 2 first; this probe had the identical construct and
# was one larger fixture away from the same silent inversion.

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

# A credential form.
if printf '%s' "$body" | grep -qiE '<input[^>]+type=["'"'"']?password'; then
  fail="$fail a password field is served where the site should be;"
fi

# AND a link offering a way in. A LINK IS A DOOR.
#
# The original comment here argued that matching the word "login" would cry wolf,
# because a nav link reading "Sign in" is normal on a public site. That reasoning
# was wrong on this surface and it cost us the actual defect: on 2026-08-09 the
# garden domain served Jeff's personal CV page with a "Login" link in the nav to
# anyone who clicked past the root — and this probe called it clean, because
# there was no form. Wren's browser scenario caught it by asserting on the
# ANCHOR, not the input.
#
# "No credential form" and "no way in from here" are different claims. I was
# asserting the weaker one while believing I had the stronger.
#
# This is the PUBLIC storefront; nothing here should offer sign-in at all, so
# there is no wolf to cry. A surface that legitimately needs a sign-in link is
# not this one.
if printf '%s' "$body" | grep -qiE '<a[^>]*>[^<]*(log ?in|sign ?in|sign ?up|register)[^<]*</a>'; then
  fail="$fail a link offering sign-in is served on the public site — a link is a door;"
fi

# And the personal site must not be reachable here at all. Naming the actual
# strings that appeared, because "wrong page" is not detectable in general but
# THIS wrong page is the one we shipped.
if printf '%s' "$body" | grep -qiE 'Engineering Leader|Systems Thinker'; then
  fail="$fail the personal site is being served on the public garden domain;"
fi

# THE SITE'S OWN IDENTITY — the garden business, by name.
#
# This check used to count links to /about, /blog and /chorus. Those are the
# PERSONAL site's navigation, and requiring them meant this probe demanded that
# the public root serve Jeff's personal site. It was green all week for exactly
# the reason it should have been red: the wrong site was at the front door, and
# the check had the wrong site written into it.
#
# So it now asserts what a garden client must see. Getting this wrong in the
# other direction is the whole reason row 1 exists — the root serving something
# other than the thing it is for.
#
# (The counting method still matters: grep -c counts matching LINES, so three
# markers on one line would score 1. -o | wc -l counts matches.)
idcount=$(printf '%s' "$body" | grep -oiE 'Light Life|Urban Gardens|Roslindale' | sort -uf | wc -l | tr -d ' ')
if [ "${idcount:-0}" -lt 2 ]; then
  fail="$fail the garden site's own name is absent (found ${idcount:-0} of Light Life / Urban Gardens / Roslindale);"
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

echo "row1 GREEN — $src serves the garden site (${bytes} bytes, ${idcount}/3 name markers, no credential form, no sign-in link, no personal site)"
