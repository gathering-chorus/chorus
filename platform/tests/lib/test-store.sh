# #4332 — the store a bats suite writes to. Never /pods.
#
# Reading all 267 bats suites on 2026-09-26 found suites that wrote to the
# production dataset (/pods) on every nightly: throwaway graphs, probe rows
# left behind, an INSERT with no gate. A test that writes prod makes the next
# red a lie, and it is the class that emptied the ontology on 2026-08-28.
#
# test_store gives a suite its own dataset: an in-memory Fuseki dataset,
# created on first use and shared by the run. Suites point their curl calls
# and athena-deploy at it through FUSEKI_GSP / FUSEKI_QUERY / FUSEKI_UPDATE.
# Reads of the real model stay on /pods (read-only).
#
#   . "$ROOT/platform/tests/lib/test-store.sh"
#   setup() { test_store || skip "UNMEASURED: $TEST_STORE_WHY"; ... }
#
# Returns non-zero (and sets TEST_STORE_WHY) when Fuseki is down or the
# dataset cannot be created, so the suite reports UNMEASURED, never green
# against the wrong store.

test_store() {
  local base="${FUSEKI_BASE_URL:-http://localhost:3030}"
  local name="${CHORUS_TEST_STORE:-chorus-test}"
  if [ "$name" = "pods" ]; then
    TEST_STORE_WHY="CHORUS_TEST_STORE=pods is the production dataset"
    return 3
  fi
  if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
    # shellcheck disable=SC1091
    . "${CHORUS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
  fi
  local auth=()
  [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && auth=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  if ! curl -sf --max-time 5 -o /dev/null "$base/\$/ping"; then
    TEST_STORE_WHY="Fuseki unreachable at $base"
    return 2
  fi
  if ! curl -sf --max-time 5 -o /dev/null "${auth[@]+"${auth[@]}"}" "$base/\$/datasets/$name"; then
    if ! curl -sf --max-time 10 -o /dev/null "${auth[@]+"${auth[@]}"}" \
        -X POST "$base/\$/datasets" --data "dbName=$name&dbType=mem"; then
      TEST_STORE_WHY="could not create the test dataset /$name"
      return 3
    fi
  fi
  export TEST_STORE="$base/$name"
  export FUSEKI_GSP="$TEST_STORE/data"
  export FUSEKI_QUERY="$TEST_STORE/query"
  export FUSEKI_UPDATE="$TEST_STORE/update"
  return 0
}

# A write URL a suite is about to use must not be the production dataset.
# Fails loudly, so a suite that drifts back to /pods goes red, not silent.
assert_not_prod() {
  case "$1" in
    */pods/*|*/pods) echo "refusing: $1 is the production dataset (#4332)" >&2; return 1 ;;
  esac
  return 0
}

# True when the variant at $1 answers from the production store. Today a werk
# variant's athena-make has no CHORUS_FUSEKI of its own, so it reads and writes
# /pods (measured 2026-09-26: prove-live's 4185 run posted 3 rows into prod and
# only the mass-delete guard stopped it deleting 6,371). The tell: it serves
# crawled CodeFile rows, which only the production store holds — the crawler
# runs against prod. A suite that writes must report UNMEASURED here, never
# write. (#4334)
variant_shares_prod() {
  local n
  n=$(curl -s --max-time 20 "$1/code/files?limit=1" \
    | python3 -c 'import sys,json; print(len(json.load(sys.stdin).get("data",[])))' 2>/dev/null)
  [ "${n:-0}" -gt 0 ]
}
