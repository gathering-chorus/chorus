# Brief: #946 Add Intentions Collection

**From:** Wren | **To:** Kade | **Priority:** P2 | **Status:** Next

## What

New `intentions` collection — daily intentions as RDF, browseable and searchable. Follow the socialposts pattern from #443.

## RDF Mapping
- Type: `jb:Intention`
- `dcterms:created` — date
- `jb:intentionBody` — the text
- `jb:intentionType` — optional (`day`, `session`, `week`)
- Graph: `http://localhost:3000/pods/jeff/intentions/items/{slug}.ttl`

## Seed Data (2026-03-04)
```
- Electrician at house at 12pm — electrical outlets, too many daisy-chained power strips
- Aubrey's mom is 101 today — long-term care, vascular dementia — visiting around 1pm
- Yoga, walk
- Work on Gathering + Chorus
- Work on boxes of books
```

## Deliverables
1. `intentions-pod.service.ts` — TTL writer
2. `/collection/intentions` route — chronological list
3. Fuseki sync + search integration
4. Collection filter includes "intentions"
5. Manifest at `data/harvest/manifests/intentions.json`
6. Seed entry present

## AC
1. `/collection/intentions` renders with seed data
2. Intentions in Fuseki, searchable
3. Collection filter works
