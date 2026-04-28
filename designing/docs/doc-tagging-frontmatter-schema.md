# Doc Catalog Frontmatter Schema (#2520)

The doc-catalog tagger infers `(product, subproduct?, subdomain?)` from path
and filename. When inference is wrong, a doc can declare its own tags via
YAML frontmatter at the top of the file. Frontmatter overrides all other
signals.

## Schema

```yaml
---
product: chorus | gathering | akasha | borg
subproduct: loom | werk | athena | convergence | clearing | quality
subdomain: <athena-subdomain-id>     # e.g., loom-decisions, photos-domain
---
```

All three keys are optional, but if any is set the entire tag returns from
inferTags with `signal: frontmatter` and `confidence: high`.

## Valid values

**product** (required if frontmatter is the override):

| value | meaning |
|---|---|
| `chorus` | Chorus product (the protocol layer) |
| `gathering` | Gathering app — blog, photos, garden, etc. |
| `akasha` | Akasha consulting site |
| `borg` | Borg shaping surface |

**subproduct** (Chorus only — Gathering and Akasha do not have subproducts):

| value | label | owns |
|---|---|---|
| `loom` | Loom | principles, decisions, policies, practices, rcas, analytics, metrics |
| `werk` | Werk | cards, roles |
| `athena` | Athena | domains, knowledge, services, integrations, ontology |
| `convergence` | Convergence | ICD harvest pipeline |
| `clearing` | The Clearing | messages, multi-role coordination |
| `quality` | Quality | gates, tests |

**subdomain** must be one of the 48 Athena subdomain IDs. Run
`curl localhost:3340/api/athena/subdomains` for the live list. Examples:
`loom-decisions`, `photos-domain`, `athena-domain`, `cards-service`.

## Conventions

- **Be conservative.** Only override frontmatter if the inference is
  genuinely wrong. The path-based and filename-based rules are already
  high-confidence for most docs.
- **Subdomain implies subproduct.** If you set `subdomain: loom-decisions`,
  the tagger backfills `subproduct: loom` automatically. You can omit
  subproduct in that case.
- **Drift is detected.** If you claim a subdomain that doesn't exist in
  Athena, `/api/doc-catalog/tags` surfaces it under `drift[]` with the
  closest valid match suggested. The doc-inventory page renders these
  as warnings.
- **Backwards-compatible.** Existing readers (e.g., the doc-inventory page)
  ignore unknown YAML keys. Adding these tags doesn't break anything.

## Example

A research doc that lives in `data/about/AI_FOUNDATIONS.html` but is really
about Chorus's Athena substrate:

```html
---
product: chorus
subproduct: athena
subdomain: knowledge-domain
---
<!DOCTYPE html>
<html>...
```

The catalog now groups it under Chorus → Athena → knowledge regardless of
where the file physically lives.

## Reference

- Production module: `platform/api/src/handlers/doc-tagger.ts`
- Drift detector: `platform/api/src/handlers/doc-tag-drift.ts`
- Coverage endpoint: `GET /api/doc-catalog/tags`
- Card: #2520
