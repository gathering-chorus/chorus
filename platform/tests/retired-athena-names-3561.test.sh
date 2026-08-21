#!/bin/bash
# @test-type: fitness
# retired-name-guard: exempt — this check must contain the retired names to search for them.
# #3561 — the retired-name guard. FAILS on any live reference to a pre-rename
# athena name. Written BEFORE the rename so it is proven red first (#3734).
#
# Jeff, 2026-08-21, after a deploy error handed him 'chorus-model-deploy':
# "i thought we were doing athena-model, athena-make (aka owl-api) and
# athena-deploy". A name nobody can rely on costs a lookup every time.
set -u
SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
# Both spellings: `owl-api` is the binary/service name, `owl_api` is the CRATE
# path in Rust `use` lines. The hyphen-only pattern reported PASS on 2026-08-21
# while twelve test files still said `owl_api::` and would not compile — one
# spelling of a name is not the name.
RETIRED="owl-api|owl_api|chorus-model-deploy"
# A guard whose search target has moved must fail LOUDLY, never pass vacuously
# (#3734). Without this, a wrong ROOT greps nothing and reports PASS.
for d in platform directing; do
  [ -d "$ROOT/$d" ] || { echo "retired-name guard: FAIL — search root $ROOT/$d does not exist; guard cannot see the tree it grades" >&2; exit 1; }
done
# Historical surfaces keep their old names on purpose: ADRs, decisions, journals
# and the retirement ledger are the RECORD of the rename, not drift. This file
# is excluded by its own path — it must contain the retired names to search for
# them, and excluding by path (not by name) keeps the exclusion honest if it moves.
hits=$(grep -rIl -E "$RETIRED" \
  --include="*.rs" --include="*.ts" --include="*.sh" --include="*.toml" --include="*.yml" \
  "$ROOT/platform" "$ROOT/directing" 2>/dev/null \
  | grep -v node_modules | grep -v "/target/" | grep -vF "$SELF" \
  | while read -r f; do
      # A guard has to contain the names it searches for. The exemption is a
      # declared marker, allowed ONLY under platform/tests, so it cannot be used
      # to quiet a real caller.
      exempt=no
      if [ "${f#"$ROOT/platform/tests/"}" != "$f" ] && grep -q "retired-name-guard: exempt" "$f"; then
        exempt=yes
      fi
      [ "$exempt" = yes ] || echo "$f"
    done | sort)
n=$(printf "%s" "$hits" | grep -c . || true)
if [ "$n" -gt 0 ]; then
  echo "retired-name guard: FAIL — $n file(s) still reference a retired athena name"
  printf "%s\n" "$hits" | head -20 | sed 's/^/  /'
  exit 1
fi

# CONTENT and FILENAMES are two states. Grepping content reported PASS on
# 2026-08-21 while `owl-api-launch.sh` and `owl-api-regen.sh` sat in the
# renamed crate — a check that cannot see the name on the file cannot separate
# "renamed" from "renamed inside only". Scoped to the verb surfaces: page routes
# under api/views and api/public keep their names until a route rename is done
# deliberately, which is separate work, not drift.
fnames=$(find "$ROOT/platform/scripts" "$ROOT/platform/tests" "$ROOT/platform/services" \
  -type f \( -name "*owl-api*" -o -name "*chorus-model*" -o -name "*chorus_model*" \) \
  -not -path "*/target/*" -not -path "*/node_modules/*" 2>/dev/null \
  | grep -vF "$SELF" | grep -v "chorus_model_retired.rs" | sort)
fn=$(printf "%s" "$fnames" | grep -c . || true)
if [ "$fn" -gt 0 ]; then
  echo "retired-name guard: FAIL — $fn file(s) still CARRY a retired athena name"
  printf "%s\n" "$fnames" | head -20 | sed 's/^/  /'
  exit 1
fi

echo "retired-name guard: PASS — no live references to, and no files named for, $RETIRED"
