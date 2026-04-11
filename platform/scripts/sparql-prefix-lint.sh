#!/usr/bin/env bash
# SPARQL prefix lint — verify every .sparql file declares all prefixes it uses (#1909)
# Exit 0 = all clean. Exit 1 = undeclared prefixes found.
set -euo pipefail

SPARQL_DIR="${1:-$(dirname "$0")/../api/src/sparql}"
ERRORS=0

for file in "$SPARQL_DIR"/*.sparql; do
  [ -f "$file" ] || continue
  basename=$(basename "$file")

  # Extract declared prefixes (PREFIX foo: <...>)
  declared=$(grep -ioE 'PREFIX\s+(\w+):' "$file" | sed 's/[Pp][Rr][Ee][Ff][Ii][Xx]//; s/[[:space:]]//g; s/://' | tr '[:upper:]' '[:lower:]' | sort -u || true)

  # Extract used prefixes (word followed by : that isn't in a URI or PREFIX line)
  # Match patterns like chorus:SubDomain, rdfs:label, owl:NamedIndividual
  # Strip URIs in angle brackets and PREFIX lines, then find word: patterns
  used=$(sed 's/<[^>]*>//g' "$file" | grep -v -i '^PREFIX' | grep -oE '\b([a-zA-Z][a-zA-Z0-9]*):' | sed 's/://' | \
    grep -v -E '^(http|https|urn|file|mailto|GRAPH|graph|AS|COUNT|DISTINCT|SELECT|WHERE|OPTIONAL|FILTER|ORDER|GROUP|HAVING|LIMIT|OFFSET|BIND|VALUES)$' | \
    tr '[:upper:]' '[:lower:]' | sort -u || true)

  # Check each used prefix is declared
  for prefix in $used; do
    if ! echo "$declared" | grep -qx "$prefix"; then
      echo "ERROR: $basename uses prefix '$prefix:' but does not declare it"
      ERRORS=$((ERRORS + 1))
    fi
  done
done

if [ "$ERRORS" -gt 0 ]; then
  echo ""
  echo "Found $ERRORS undeclared prefix(es). Add PREFIX declarations to the .sparql files."
  exit 1
else
  echo "SPARQL prefix lint: all files clean"
  exit 0
fi
