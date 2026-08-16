#!/usr/bin/env bash
# GREEN = a present-but-invalid token is refused (fail closed, never degrades).
out=$(CHORUS_IDENTITY_TOKEN="bogus-token-3900" "$HOME/.chorus/bin/athena-model" add --kind role --name probe-denied 2>&1)
[ $? -ne 0 ] && echo "$out" | grep -q "identity-token-invalid"
