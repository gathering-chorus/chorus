#!/usr/bin/env bash
# GREEN = every actor in the roster has a 0600 nostr key. RED names the missing.
missing=0
for who in silas wren kade bridge jeff canary marknakib; do
  f="$HOME/.chorus/identity/$who/nostr.json"
  if [ ! -f "$f" ]; then echo "MISSING key: $who"; missing=1
  elif [ "$(stat -f %Lp "$f")" != "600" ]; then echo "BAD MODE key: $who"; missing=1; fi
done
exit $missing
