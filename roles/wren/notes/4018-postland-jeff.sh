#!/bin/bash
# #4018 post-land, Jeff's half: after 4018-postland.py rewrites pulse and the Clearing, put their
# consumes links back as real links (the old door would save them as strings, #4478), plus pulse's
# card.pulled event. Uses jeff-products-write (granted 2026-10-10 08:52). Reads back through the door.
set -euo pipefail
ROOT=/Users/jeffbridwell/CascadeProjects/chorus
TOK=$(bash "$ROOT/platform/scripts/chorus-identity-token" jeff)
C='https://jeffbridwell.com/chorus#'
BODY=""
for d in alerts cards code events knowledge logs memory messages monitors roles security streams tests; do
  BODY+=$(printf 'INS\t<%spulse>\t<%sconsumes>\t<%s%s>' "$C" "$C" "$C" "$d")$'\n'
done
BODY+=$(printf 'INS\t<%spulse>\t<%sconsumesEvent>\t<%seventtype-card-pulled>' "$C" "$C" "$C")$'\n'
for d in identity roles; do
  BODY+=$(printf 'INS\t<%sclearing>\t<%sconsumes>\t<%s%s>' "$C" "$C" "$C" "$d")$'\n'
done
curl -s -X POST http://127.0.0.1:3360/batch \
  -H "Authorization: Bearer $TOK" -H 'x-target-graph: urn:chorus:domains:products' -H 'Content-Type: text/plain' \
  --data-binary "$BODY" | head -c 300
echo
for p in pulse clearing; do
  curl -s "http://127.0.0.1:3360/v1/products/products/$p" | python3 -c "import json,sys; l=json.load(sys.stdin)['links']; print('$p', 'consumes', len(l.get('consumes', [])), '| hasDomain', l.get('hasDomain'))"
done
