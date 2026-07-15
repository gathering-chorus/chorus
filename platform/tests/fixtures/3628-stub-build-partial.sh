#!/usr/bin/env bash
# #3628 test stub — a "partial" SDK build: emit.js only, token.js missing
# (the exact #3619 miss the pre-swap completeness check must refuse).
echo "new" > "$CHORUS_SDK_BUILD_OUT/emit.js"
