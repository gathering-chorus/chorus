#!/usr/bin/env bash
# GREEN = the allow-set the doors read has >= 7 principals with webIds.
n=$(curl -sf --max-time 10 "http://localhost:3030/pods/query" -H 'Accept: text/csv' \
  --data-urlencode "query=SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <urn:chorus:domains:security> { ?p <https://jeffbridwell.com/chorus#webId> ?w } }" | tail -1 | tr -dc 0-9)
echo "allow-set principals: ${n:-0}"
[ "${n:-0}" -ge 7 ]
