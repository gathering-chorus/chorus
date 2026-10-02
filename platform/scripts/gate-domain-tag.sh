#!/usr/bin/env bash
# @domain: tests
# gate-domain-tag.sh (#4419) — a NEW code or test file must carry a domain.
#
# Jeff, 2026-10-01: blast radius and per-card test selection "require in synch
# domains that are tagged properly". The domain is what the crawler gives the
# file (its own rules: the file's @domain header, its unit, its tree, its
# directory). A new file no rule places is refused here, before it lands with
# no domain and its tests can never be found by the domain it touches.
#
#   gate-domain-tag.sh staged        # files ADDED in the index
#   gate-domain-tag.sh <file>...     # explicit files
# Exit 0 = all placed (or nothing to check); 1 = an untagged new file;
# fail-open (0, with a line) when no crawler answers the --domains-of seam.
# Seam: DOMAIN_TAG_CRAWL (the crawler binary to ask).
set -u
REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO" || exit 0
if [ "${1:-}" = "staged" ]; then
  files=$(git diff --cached --name-only --diff-filter=A)
else
  files=$(printf '%s\n' "$@")
fi
files=$(printf '%s\n' "$files" | grep -E '\.(rs|ts|tsx|js|cjs|mjs|sh|bats|py|feature)$' \
  | grep -vE '(^|/)(node_modules|target|dist|fixtures|\.tmp-[^/]*)/' || true)
[ -n "$files" ] || exit 0
answer=""
for bin in "${DOMAIN_TAG_CRAWL:-}" "$REPO/platform/services/chorus-crawl/target/release/chorus-crawl" chorus-crawl; do
  [ -n "$bin" ] || continue
  # shellcheck disable=SC2086
  answer=$("$bin" --domains-of $files 2>/dev/null) && [ -n "$answer" ] && break
  answer=""
done
if [ -z "$answer" ]; then
  echo "domain-tag: no crawler answered --domains-of — not checked (#4419)" >&2
  exit 0
fi
untagged=$(printf '%s\n' "$answer" | awk -F'\t' '$2==""{print $1}')
[ -z "$untagged" ] && exit 0
echo "domain-tag: new file(s) with no domain — no rule places them (#4419):" >&2
printf '  %s\n' $untagged >&2
exit 1
