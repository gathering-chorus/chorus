#!/usr/bin/env bash
# GREEN = the SERVED vocabVersion equals the ledger head — the hand-edit
# detector (#3902). The ledger (designing/schemas/model-version-ledger.jsonl)
# is the one home; the store projection and every athena-make envelope must agree
# with it. A mismatch means someone edited the projection or the store around
# the pen — exactly the drift this versioning program exists to end.
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
LEDGER="$R/designing/schemas/model-version-ledger.jsonl"
[ -f "$LEDGER" ] || { echo "ledger missing at $LEDGER"; exit 1; }
head_v=$(grep -o '"version": *"[0-9.]*"' "$LEDGER" | tail -1 | grep -o '[0-9.]*[0-9]')
[ -n "$head_v" ] || { echo "ledger head unparseable"; exit 1; }
served=$(curl -sf --max-time 10 "http://localhost:3360/domains" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin).get("vocabVersion",""))' 2>/dev/null)
echo "ledger=$head_v served=${served:-<absent>}"
[ "$served" = "$head_v" ]
