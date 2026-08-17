#!/usr/bin/env bash
# GREEN = git history carries no LIVE credential — the #3386 disposition holds.
#
# The 35 baselined findings were all the pre-#3611 stock `admin:admin` (dead
# since the credential rotated to a 20-char value in a 0600 env file). That
# verdict is only durable if a NEW long credential entering history reds
# something. This probe is that something: it re-runs the disposition's own
# check every night instead of trusting a document written once.
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
cd "$R" 2>/dev/null || { echo "chorus root unreadable"; exit 1; }

# 1. No historical blob of the credential-bearing logs may hold a secret longer
#    than the stock default. (Length is the discriminator: `admin` is dead, a
#    20-char value is not.)
BAD=0
for f in platform/logs/permission-prompts.log ops/logs/permission-prompts.log; do
  git log --all --format=%H -- "$f" 2>/dev/null | while read -r c; do
    n=$(git show "$c:$f" 2>/dev/null | grep -cE '\-u +[A-Za-z0-9_-]+:[A-Za-z0-9]{6,}')
    [ "${n:-0}" -gt 0 ] && echo "LONG-CRED in $c:$f ($n lines)"
  done
done | grep . && BAD=1

# 2. The live credential must not be the historical one (rotation still holds).
CRED="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
if [ -r "$CRED" ] && grep -qE '^FUSEKI_ADMIN_PASSWORD=admin$' "$CRED"; then
  echo "LIVE CRED REGRESSED to the historical default"
  BAD=1
fi

[ "$BAD" -eq 0 ] && echo "history disposition holds: no live credential in history"
exit "$BAD"
