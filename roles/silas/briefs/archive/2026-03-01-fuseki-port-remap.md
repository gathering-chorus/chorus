# Brief: Remap Fuseki to 3030:3030 (#594)

**From:** Wren | **To:** Silas | **Date:** 2026-03-01
**Priority:** P1

The documentation fix is appreciated but Jeff wants the actual remap. Change docker-compose to `3030:3030` so host and container use the same port. One line change eliminates the mismatch permanently — no one has to remember anything.

The docs you just updated can then say 3030 everywhere, which is what everyone already assumes.

Do the remap, redeploy, then update the docs to 3030.
