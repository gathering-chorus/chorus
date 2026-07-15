#!/usr/bin/env bash
# #3628 test stub — a "good" SDK build: writes every runtime module into the
# temp outDir the deploy script exports as CHORUS_SDK_BUILD_OUT.
echo "new" > "$CHORUS_SDK_BUILD_OUT/emit.js"
echo "new" > "$CHORUS_SDK_BUILD_OUT/token.js"
