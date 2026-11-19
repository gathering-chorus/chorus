#!/usr/bin/env bats
# @test-type: integration — reads card #4966's story from the Chorus API on this machine, and the gate's source; nothing public
# @domain: identity
# @card: #4966
#
# AC1 (reworded 2026-11-16 on Jeff's "keep going on 4966"): what the logs can name is named on
# the card, with evidence, before any fix; the one they cannot name, the phone arriving with no
# sign-in, is recorded from this land on by the gate's refusal line (why, path, browser).
#
# Covers: directing/clearing-v2/src/share-session.ts
# Covers: directing/clearing-v2/src/server.ts

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  GATE="$ROOT/directing/clearing-v2/src/share-session.ts"
  SERVER="$ROOT/directing/clearing-v2/src/server.ts"
}

# reads a card story on stdin; exit 0 when one comment names causes and another says what is not named.
# (the story carries the first 500 characters of a comment, so each fact has its own short comment)
causes_named() {
  python3 -c '
import json, sys
story = json.load(sys.stdin)
texts = [str(i.get("text", "")) for i in story.get("timeline", []) if i.get("source") == "vikunja"]
named = any(all(n in t for n in ["AC1 evidence", "NAMED", "past the sign-in guard"]) for t in texts)
unnamed = any(all(n in t for n in ["AC1 NOT NAMED", "no sign-in at all", "recorded by the gate"]) for t in texts)
sys.exit(0 if named and unnamed else 1)'
}

@test "the card names what the logs can name, and says plainly what they cannot" {
  # the card story from the Chorus API (comments included); a role account has no board token
  card="$(curl -sf --max-time 15 "${CHORUS_API:-http://localhost:3340}/api/chorus/card-story/4966")" || { echo "card-story unreadable"; return 1; }
  printf '%s' "$card" | causes_named || { echo "no card comment names the causes"; return 1; }
}

@test "NEGATIVE PROOF: the same story without the comment saying what is not named does not pass" {
  card="$(curl -sf --max-time 15 "${CHORUS_API:-http://localhost:3340}/api/chorus/card-story/4966")" || { echo "card-story unreadable"; return 1; }
  stripped="$(printf '%s' "$card" | python3 -c '
import json, sys
d = json.load(sys.stdin)
d["timeline"] = [i for i in d.get("timeline", []) if "AC1 NOT NAMED" not in str(i.get("text", ""))]
print(json.dumps(d))')"
  run causes_named <<< "$stripped"
  [ "$status" -ne 0 ]
}

@test "the case the logs cannot name is recorded from now on: every refused request logs why and the browser" {
  grep -q 'gate.refused' "$GATE"
  grep -q 'why=\${why}' "$GATE"
  # and which request and which browser: the line carries the method, the path and the user agent
  # (#5015 AC4 put host, scheme and cookie names between the path and the browser)
  grep -qE 'why=\$\{why\} \$\{method\} \$\{path\} host=.* ua="' "$GATE"
  # the server tells the gate why: no cookie, or the cookie verdict'"'"'s own reason
  grep -q "if (!raw) return 'no-cookie';" "$SERVER"
  grep -q 'return v.ok ? .unknown. : v.reason;' "$SERVER"
}

@test "NEGATIVE PROOF: a gate that records no reason fails" {
  work="$BATS_TEST_TMPDIR/share-session.ts"
  sed 's/why=\${why}//' "$GATE" > "$work"
  if cmp -s "$GATE" "$work"; then echo "mutation did not apply"; return 1; fi
  run grep -q 'why=\${why}' "$work"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: a gate line with no path or no browser fails" {
  for drop in ' \${method} \${path}' ' ua="\${ua.replace(\/"\/g, "'"'"'").slice(0, 120)}"'; do
    work="$BATS_TEST_TMPDIR/share-session.ts"
    sed "s/$drop//" "$GATE" > "$work"
    if cmp -s "$GATE" "$work"; then echo "mutation did not apply: $drop"; return 1; fi
    run grep -qE 'why=\$\{why\} \$\{method\} \$\{path\} host=.* ua="' "$work"
    [ "$status" -ne 0 ] || return 1
  done
}
