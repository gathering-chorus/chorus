#!/bin/bash
# #4064 — the commitments check. Reads chorus:Commitment rows and answers the
# question the rows exist for: "which promise in a service design is on nobody's
# card?" Two rules, each a FAIL line:
#   1. an OPEN commitment with no chorus:card is unowned work           -> FAIL open-no-card
#   2. a CLOSED commitment whose chorus:probe runs and exits non-zero   -> FAIL closed-probe-red
# A closed commitment with no probe, or a probe that is not executable, is
# UNMEASURED (printed, never a pass). Exit 1 on any FAIL, else 0.
# Seams: COMMITMENT_TTL (file, default the security rows), COMMITMENT_PROBE_ROOT
# (where relative probe paths resolve).
set -u
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
TTL="${COMMITMENT_TTL:-$ROOT/roles/silas/ontology/security-commitments-4064.ttl}"
PROBE_ROOT="${COMMITMENT_PROBE_ROOT:-$ROOT}"
[ -r "$TTL" ] || { echo "FAIL no-ttl $TTL is not readable"; exit 1; }
command -v sparql >/dev/null || { echo "UNMEASURED sparql (jena) not on PATH"; exit 2; }

Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?c ?status ?card ?probe WHERE {
  ?c a chorus:Commitment ; chorus:status ?status .
  OPTIONAL { ?c chorus:card ?card } OPTIONAL { ?c chorus:probe ?probe }
} ORDER BY ?c'
rows=$(sparql --data "$TTL" --results TSV --query <(echo "$Q") 2>&1) || { echo "FAIL ttl-unparseable $rows" | head -3; exit 1; }

fail=0; n=0
while IFS=$'\t' read -r c status card probe; do
  [ -z "$c" ] && continue
  n=$((n+1)); id=${c##*#}; id=${id%>}; status=${status//\"/}; probe=${probe//\"/}
  case "$status" in
    open)
      if [ -z "$card" ]; then echo "FAIL open-no-card $id"; fail=1; else echo "ok   open-carded  $id ${card##*#}"; fi ;;
    closed)
      if [ -z "$probe" ]; then echo "UNMEASURED closed-no-probe $id"; continue; fi
      # a probe is a model row (an IRI such as chorus:security-probe-...) or a runnable path;
      # an IRI cannot be executed here — say so rather than pass it
      case "$probe" in http://*|https://*|urn:*|\<*) echo "UNMEASURED closed-probe-is-model-ref $id ${probe##*#}"; continue ;; esac
      p="$probe"; [ "${p:0:1}" = / ] || p="$PROBE_ROOT/$p"
      if [ ! -x "$p" ]; then echo "UNMEASURED closed-probe-missing $id $probe"; continue; fi
      if "$p" >/dev/null 2>&1; then echo "ok   closed-probe-green $id"; else echo "FAIL closed-probe-red $id $probe"; fail=1; fi ;;
    deferred) echo "ok   deferred $id" ;;
    *) echo "FAIL bad-status $id '$status'"; fail=1 ;;
  esac
done < <(printf '%s\n' "$rows" | tail -n +2)
[ "$n" -gt 0 ] || { echo "FAIL no-commitments $TTL holds zero chorus:Commitment rows"; exit 1; }
echo "commitments=$n fail=$fail"
exit $fail
