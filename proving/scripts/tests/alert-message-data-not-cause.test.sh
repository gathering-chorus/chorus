#!/usr/bin/env bash
# #3519 — Alert CONTRACT: an alert's action message reports the OBSERVATION
# (what was measured), never an inferred cause or a prescribed fix.
set -uo pipefail
ALERT_DIR="${ALERT_DIR:-$(cd "$(dirname "$0")/../../domains/alerts" && pwd)}"
BANNED='likely|probably|may be|might be|expired|regenerate|Fix:|restart|reboot|check nifi|check ollama|\.env|broken'
fails=0; checked=0
for f in "$ALERT_DIR"/*.yml; do
  [ -f "$f" ] || continue
  name=$(grep -m1 '^name:' "$f" | sed 's/name: *//')
  # action sends: "$OPS_NUDGE" <role> "MESSAGE"  — pull MESSAGE(s)
  while IFS= read -r msg; do
    [ -z "$msg" ] && continue
    checked=$((checked+1))
    if echo "$msg" | grep -qiE "$BANNED"; then
      echo "FAIL  $name — message asserts cause/fix:"
      echo "        \"$msg\""
      fails=$((fails+1))
    fi
  done < <(grep -E '\$OPS_NUDGE" +(silas|kade|wren)' "$f" | sed -E 's/.*(silas|kade|wren)" * "([^"]*)".*/\2/')
done
echo "---"
echo "checked=$checked  violations=$fails"
[ "$fails" -eq 0 ] && { echo "PASS: every alert message reports observation, not cause/fix"; exit 0; }
echo "RED: $fails alert(s) assert a cause or prescribe a fix in the message"
exit 1
