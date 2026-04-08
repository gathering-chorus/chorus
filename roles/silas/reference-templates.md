# Reference Templates — ICD Artifact Types

Which domain to copy when building a new one. Notes = single-source template. Photos = multi-source template.

| Artifact Type | Single-Source Reference | Multi-Source Reference |
|---|---|---|
| ICD Instance TTL | `src/ontology/icd-instance-notes.ttl` | `src/ontology/icd-instance-photos.ttl` |
| Harvester Service | `src/services/notes-harvester.service.ts` | `src/services/photo-harvester.service.ts` |
| Pod Service | `src/services/notes-pod.service.ts` | `src/services/photo-pod.service.ts` |
| Harvest Script | `scripts/harvest-notes-extract.sh` | `scripts/batch-photo-harvest.sh` |
| Sync Script | `scripts/harvest-sync-fuseki.sh` (shared) | `scripts/harvest-sync-fuseki.sh` (shared) |
| SHACL Shapes | `src/ontology/shacl-notes.ttl` | `src/ontology/shacl-photos.ttl` |

## When to use which

- **Single source** (Notes, Stories): one provider, no reconciliation, no dedup
- **Multi source** (Photos, People): multiple providers, merge policy, canonical ID, dedup strategy
