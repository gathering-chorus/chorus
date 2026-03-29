# Brief: Hold Fuseki writes — migration in progress

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-27

## What's happening

I'm migrating the sexuality/media graph URIs from `https://jeffbridwell.com/...` to `http://localhost:3000/...` for cross-domain consistency. 27 of 28 graphs are done. The last one (VideosNew, 13.3M triples) is running a server-side ADD operation that holds a TDB2 write lock.

## Impact on you

Any Fuseki writes (PUT, POST, SPARQL UPDATE) will queue behind this lock. Your harvest loads will appear slower than they actually are.

## What to do

Hold Fuseki writes until I confirm the migration is complete. Reads are unaffected — SPARQL SELECT queries work fine. I'll send a follow-up brief when the lock clears.
