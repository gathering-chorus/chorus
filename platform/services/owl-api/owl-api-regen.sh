#!/usr/bin/env bash
# #3488 — regenerate a class's owl-api surfaces and LAND them at the
# model-declared repo location. The location is config-as-data: owl-api reads
# chorus:repoTarget (or derives <vs-step>/products/<product>/domains/<domain>
# from the containment edges) and `generate-target` prints it. This script is
# the WRITE half — "rebuild <class>" → the generated artifacts appear at their
# proper repo home, not a hardcoded dir. The LOCATION half of "generated APIs
# land in the repo where they belong" (Jeff, #3488).
#
# Usage: owl-api-regen.sh [Class]   (default: Domain)
# Env:   OWL_API_BIN (default: owl-api on PATH), CHORUS_FUSEKI, CHORUS_ROOT
set -euo pipefail

CLASS="${1:-Domain}"
BIN="${OWL_API_BIN:-owl-api}"
# Land in the worktree we're operating in (the card's werk during a card,
# canonical when run there) — NOT a pinned CHORUS_ROOT, which points at
# canonical and would write the werk's artifacts into canonical (#3488 bug).
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "${CHORUS_ROOT:-$PWD}")"

# The model says WHERE this class's artifacts land.
DIR="$("$BIN" generate-target --class "$CLASS")"
if [ -z "$DIR" ]; then
  echo "owl-api-regen: no repo target resolved for $CLASS" >&2
  exit 1
fi
DEST="$ROOT/$DIR"
mkdir -p "$DEST"

# Project each surface to its file at the declared home.
"$BIN" generate           --class "$CLASS" > "$DEST/routes.json"
"$BIN" generate-openapi   --class "$CLASS" > "$DEST/openapi.json"
"$BIN" generate-page      --class "$CLASS" > "$DEST/index.html"
"$BIN" generate-dashboard --class "$CLASS" > "$DEST/dashboard.json"
"$BIN" generate-tests     --class "$CLASS" > "$DEST/tests.json"
"$BIN" generate-mcp       --class "$CLASS" > "$DEST/mcp.json"

echo "landed $CLASS → $DIR"
ls -1 "$DEST"

# #3488 — BIND into the product API: a domain's API auto-registers into its
# product's API by construction (derived from the product's hasDomain edges).
# Regenerate the product index at the product home so the binding follows the
# graph — no manual register step. (Skipped if the path isn't under products/.)
if echo "$DIR" | grep -q '/products/'; then
  PROD_HOME="$(echo "$DIR" | sed -E 's#(.*/products/[^/]+)/.*#\1#')"
  PRODUCT="$(basename "$PROD_HOME")"
  "$BIN" generate-product --product "$PRODUCT" > "$ROOT/$PROD_HOME/index.json"
  echo "bound $CLASS into product '$PRODUCT' → $PROD_HOME/index.json"
fi
