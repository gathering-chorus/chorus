#!/usr/bin/env bash
# write-story.sh — capture a Jeff-told story into the TTL story graph (#2321).
#
# One graph per story, one file shape, one write path. stories.md is
# deprecated; this is the replacement tool the story_write_gate hook
# redirects to.
#
# Usage:
#   write-story.sh "Title" "What he said" "What it tells us" "Where it applies"
#
# Emits:
#   - SPARQL INSERT DATA into urn:jb/jeff/stories/<slug>.ttl in Fuseki /pods
#   - spine event story.captured
#
# Exit codes:
#   0 — story written
#   1 — bad args
#   2 — Fuseki write failed

set -euo pipefail

FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log"
ROLE="${ROLE:-$(whoami)}"

# #3630 — carry the Fuseki write credential on writes (empty unless
# FUSEKI_ADMIN_PASSWORD is set; harmless until shiro requires auth →
# deploy-before-require). Same #3566 helper the other writers source.
source "$(dirname "${BASH_SOURCE[0]}")/fuseki-auth.sh"

usage() {
  cat <<EOF >&2
Usage: write-story.sh "Title" "What he said" "What it tells us" "Where it applies"

Captures a story as a jeff:Story instance in Fuseki at
urn:jb/jeff/stories/<slug>.ttl where <slug> is a kebab-cased form of Title.

All four fields are required. Each ends up in the story body joined with
the canonical three-section heading shape used in stories.md historically:
"What he said", "What it tells us", "Where it applies".

Examples:
  write-story.sh "Garden Learning" \\
    "Mom loves gardens, I'm like that too" \\
    "Enthusiasm-first, names-later learning style" \\
    "Product prioritization follows his attention"
EOF
}

if [ "$#" -ne 4 ]; then
  usage
  exit 1
fi

TITLE="$1"
SAID="$2"
TELLS="$3"
APPLIES="$4"

if [ -z "$TITLE" ] || [ -z "$SAID" ] || [ -z "$TELLS" ] || [ -z "$APPLIES" ]; then
  echo "ERROR: all four fields must be non-empty" >&2
  usage
  exit 1
fi

# Slug: lowercase, replace anything that isn't a-z 0-9 with -, squeeze dashes, trim.
SLUG=$(printf '%s' "$TITLE" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//')

if [ -z "$SLUG" ]; then
  echo "ERROR: title produced empty slug" >&2
  exit 1
fi

GRAPH="urn:jb/jeff/stories/${SLUG}.ttl"
CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
PERIOD="$(date -u +%Y-%m-%d)"
SUBJECT="urn:jb/jeff/stories/${SLUG}"

# Escape a string for SPARQL triple-quoted literal.
# Backslashes and triple-quotes are the only hazards in """ strings.
sparql_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"""/\\"\\"\\"/g'
}

BODY="**What he said:** ${SAID}

**What it tells us:** ${TELLS}

**Where it applies:** ${APPLIES}"

TITLE_E="$(sparql_escape "$TITLE")"
SLUG_E="$(sparql_escape "$SLUG")"
BODY_E="$(sparql_escape "$BODY")"

UPDATE=$(cat <<SPARQL
PREFIX jeff: <https://jeffbridwell.com/ontology#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
INSERT DATA {
  GRAPH <${GRAPH}> {
    <${SUBJECT}> a jeff:Story ;
      jeff:title """${TITLE_E}""" ;
      jeff:slug """${SLUG_E}""" ;
      jeff:body """${BODY_E}""" ;
      jeff:created "${CREATED}"^^xsd:dateTime ;
      jeff:period "${PERIOD}"^^xsd:date ;
      jeff:storySource "session" ;
      jeff:hasVisibility "private" .
  }
}
SPARQL
)

HTTP_CODE=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/write-story-resp.txt -w '%{http_code}' \
  -X POST \
  -H "Content-Type: application/sparql-update" \
  --data-binary "$UPDATE" \
  "$FUSEKI_UPDATE")

if [ "$HTTP_CODE" != "200" ] && [ "$HTTP_CODE" != "204" ]; then
  echo "ERROR: Fuseki update failed (HTTP $HTTP_CODE)" >&2
  cat /tmp/write-story-resp.txt >&2 || true
  exit 2
fi

echo "OK: wrote story"
echo "  graph:   ${GRAPH}"
echo "  subject: ${SUBJECT}"
echo "  title:   ${TITLE}"

# Spine event so Borg/Chorus see the write.
if [ -x "$CHORUS_LOG" ]; then
  "$CHORUS_LOG" story.captured "$ROLE" "slug=${SLUG}" "graph=${GRAPH}" >/dev/null 2>&1 || true
fi
