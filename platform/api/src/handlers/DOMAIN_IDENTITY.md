# Domain identity resolver

Single source of truth for "what does this domain id encompass across cards,
commits, graph URIs, alert files, and aliased parents." Every section handler
that backs a fold on `domain-detail.html` consumes it via `resolveDomainIdentity(id)`.

**File:** `domain-identity.ts` | **Tests:** `tests/handlers/domain-identity.test.ts` | **Ships with:** #2430

## The contract

```ts
interface DomainIdentity {
  primary: string;              // canonical domain id, kebab-case, stripped of -domain
  aliases: string[];             // alternative ids used in card tags, commits, filesystem
  domainUri: string;          // full chorus URI for Athena exact-match queries
  cardDomainTags: string[];      // `domain:X` labels to match in card search
  cardSequenceTags: string[];    // `sequence:X` labels to match in card search
  alertFileTokens: string[];     // tokens to match in alert filenames
  ontologyGraph: string;         // usually 'urn:chorus:ontology' or 'urn:chorus:instances'
}

function resolveDomainIdentity(idInput: string): DomainIdentity
```

Input normalized at entry: lowercased, underscore → kebab, `-service`/`-analytics` NOT
stripped (those are genuine domain suffixes, not scope markers). Only `-domain` is
stripped for registry lookup so `seeds-domain` and `seeds` resolve the same.

## The two URL bases

Handlers split into two shapes. The resolver serves both from one call:

| URL base | Path pattern | Resolution | Example consumers |
|----------|--------------|------------|-------------------|
| `/api/athena/domains/:id/...` | exact domain id → `chorus#<id>` URI, queries `urn:chorus:instances` | uses `domainUri` | cards, coverage, code, pages, endpoints, alerts (athena path) |
| `/api/chorus/domain/:name/...` | domain name → alias expansion → filter by `domain:X` and `sequence:X` tags | uses `primary + aliases + cardDomainTags + cardSequenceTags` | tests, decisions, releases, logs, infra |

When a handler touches both (rare), pass the full struct — it carries everything.

## When to add a registry entry

Default behavior (no registry entry) assumes the domain id IS its card tag and needs
no aliases. That works for most domains (seeds-domain, chorus-domain, photos-domain,
etc.) where cards are tagged `domain:seeds`, `domain:photos`, etc.

**Add an entry when:**
- Cards for this domain use a DIFFERENT tag (e.g., loom-principles cards use `sequence:loom`, not `sequence:loom-principles`)
- The domain is a sub-product of a parent (loom-* → loom)
- The domain renames to something its commit messages still reference by the old name

**Don't add an entry when:**
- It's a clean 1:1 between id and tag (most domains)
- You're tempted to put `-domain` stripping in the registry (default handles that)

## Adding a new domain — 3 steps

1. **Pick the id** (kebab-case, conventionally ending `-domain` if it's a top-level domain, or not for sub-products like `loom-principles`).
2. **Check if default behavior suffices:** run `cards list --label domain:<id>` vs `cards list --label sequence:<parent>`. If cards are tagged under a parent, add an entry:
   ```ts
   'your-new-domain': { aliases: ['parent'], cardSequenceTags: ['parent', 'your-new-domain'] }
   ```
3. **Validate:** fetch `/api/athena/domains/your-new-domain/cards` — count should match the cards you'd expect.

Tests in `domain-identity.test.ts` cover normalization edge cases. Add a registry-entry
test if your domain introduces a new alias shape.

## See also

- #2430 — this card
- `athena-domain-cards.ts` — first conversion, template for string-based handlers
- `domain-facets.ts fetchDomainDecisions` — second conversion, template for graph-filter handlers
- `chorus-domain-releases.ts` — third conversion, template for git-log handlers
