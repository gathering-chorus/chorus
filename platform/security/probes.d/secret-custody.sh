#!/usr/bin/env bash
# GREEN = shared secrets exist only at 0600 (room secret, css creds).
bad=0
for f in "$HOME/.chorus/buzz/room-secret" "$HOME"/.chorus/identity/*/cred.json; do
  [ -f "$f" ] || continue
  [ "$(stat -f %Lp "$f")" = "600" ] || { echo "BAD MODE: $f"; bad=1; }
done
exit $bad
