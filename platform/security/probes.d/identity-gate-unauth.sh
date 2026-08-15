#!/usr/bin/env bash
# GREEN = the DAL refuses an unauthenticated write, naming the gate.
out=$(env -u CHORUS_IDENTITY_TOKEN "$HOME/.chorus/bin/athena-model" add --kind role --name probe-denied 2>&1)
[ $? -ne 0 ] && echo "$out" | grep -q "identity-token-required"
