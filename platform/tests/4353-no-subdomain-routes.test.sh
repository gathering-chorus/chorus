#!/usr/bin/env bash
# @test-type: unit — greps the tree; no live services
#
# #4353 — SubDomain is retired. No code path may request /api/athena/subdomains,
# written out or built from pieces ("/subdomains/" + id). Comments, docs, history
# and this test itself are allowed to name it.
set -u
ROOT="${CHORUS_ROOT_OVERRIDE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1
hits=$(git grep -nE "api/athena/subdomains|['\"\`/]subdomains/['\"\`$]|/subdomains\?|ATHENA *\+ *['\"]/subdomains" -- \
  ':!*.md' ':!*.jsonl' ':!*.log' ':!*.backup' ':!*.svg' ':!designing/docs/**' ':!**/fixtures/**' \
  ':!platform/tests/4353-no-subdomain-routes.test.sh' \
  ':!platform/api/public/loom/principles-reference-impl.html' ':!platform/api/public/loom/cookbook-substrate-class-domain.html' \
  | grep -vE ':[0-9]+:\s*(#|//|\*|/\*|<!--)' || true)
n=$(printf '%s' "$hits" | grep -c . || true)
if [ "$n" -eq 0 ]; then echo "PASS no code path requests /api/athena/subdomains"; echo "=== Results: 1 passed, 0 failed ==="; exit 0; fi
echo "FAIL $n line(s) still request /api/athena/subdomains:"; printf '%s\n' "$hits" | head -40
echo "=== Results: 0 passed, 1 failed ==="; exit 1
