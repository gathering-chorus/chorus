/* eslint-disable security/detect-object-injection -- Indexing on validated SPARQL binding keys. */
/**
 * domain-identity.ts — #2430
 *
 * One resolver for every fold on domain-detail.html. Every section handler
 * (cards, tests, decisions, releases, coverage, code, pages, endpoints,
 * services, alerts, logs, prior-art, actors, scenarios, contract,
 * integrations, persistence, pipeline, gaps) calls `resolveDomainIdentity(id)`
 * to find out what the subdomain encompasses — instead of each inventing its
 * own alias table, prefix stripping, or exact-URI matching.
 *
 * Contract:
 *   - Input `id` may be kebab-case (`loom-principles`) or underscore
 *     (`loom_principles`) or trailing-suffix (`loom-principles-domain`,
 *     `tests-domain`). The resolver normalizes to kebab.
 *   - Registry entries override defaults for subdomains whose cards are
 *     tagged with a parent product tag (e.g., loom-principles cards carry
 *     `sequence:loom`, not `sequence:loom-principles`).
 *   - Subdomains without a registry entry get a sensible default identity
 *     derived from the id itself.
 */

export interface DomainIdentity {
  /** Canonical kebab id: `loom-principles` */
  primary: string;
  /** Parent tags this subdomain folds into (e.g., `['loom']` for `loom-principles`) */
  aliases: string[];
  /** TTL subject URI for Athena exact-match queries */
  subdomainUri: string;
  /** `domain:X` labels to match in card search */
  cardDomainTags: string[];
  /** `sequence:X` labels to match in card search */
  cardSequenceTags: string[];
  /** Filename tokens to match for alert scans (e.g., `['loom', 'principles']`) */
  alertFileTokens: string[];
  /** Fuseki graph — `urn:chorus:ontology` for Chorus subdomains, `urn:chorus:instances` for instance-backed folds */
  ontologyGraph: string;
}

/** Shape of a registry entry — all fields optional, defaults apply for any missing. */
interface RegistryEntry {
  aliases?: string[];
  cardDomainTags?: string[];
  cardSequenceTags?: string[];
  alertFileTokens?: string[];
  ontologyGraph?: string;
}

/**
 * Registry — only enter subdomains whose cards are tagged with a parent,
 * not their primary id. If `cards list --label domain:<id>` already returns
 * what you want, no entry needed.
 */
const REGISTRY: Record<string, RegistryEntry> = {
  // Loom sub-subdomains: cards carry `sequence:loom`, decisions carry `domain:loom`.
  'loom-principles': { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-policies':   { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-practices':  { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-decisions':  { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-metrics':    { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-analytics':  { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  'loom-rcas':       { aliases: ['loom'], cardSequenceTags: ['loom'], cardDomainTags: ['chorus', 'loom'] },
  // Quality/code/gates folds — pre-existing special cases.
  'tests': { aliases: ['quality'], cardDomainTags: ['quality'] },
  'code':  { aliases: ['code'],    cardDomainTags: ['code'] },
  'gates': { aliases: ['gates'],   cardDomainTags: ['gates'] },
};

/**
 * Normalize any incoming id to canonical kebab form.
 * Strips trailing `-domain` suffix ONLY — `-analytics`, `-service`, etc. are
 * legitimate kebab words in real subdomain names (`loom-analytics`,
 * `pulse-service`). Earlier code in athena-subdomain-cards.ts stripped too
 * aggressively and would have silently collapsed `loom-analytics` to `loom`.
 * Converts underscores to hyphens, lowercases.
 */
function normalizeId(input: string): string {
  return input
    .replace(/_/g, '-')
    .replace(/-domain$/i, '')
    .toLowerCase();
}

/** Build alertFileTokens default from a kebab id (`loom-principles` → `['loom', 'principles']`). */
function defaultAlertTokens(primary: string): string[] {
  return primary.split('-').filter((t) => t.length > 0);
}

/**
 * Resolve a subdomain id into its identity envelope.
 * Accepts kebab, underscore, or trailing-suffix forms.
 */
export function resolveDomainIdentity(rawId: string): DomainIdentity {
  const primary = normalizeId(rawId);
  const entry = REGISTRY[primary] ?? {};

  const aliases = entry.aliases ?? [];
  const cardDomainTags = entry.cardDomainTags ?? [primary];
  const cardSequenceTags = entry.cardSequenceTags ?? [];
  const alertFileTokens = entry.alertFileTokens ?? defaultAlertTokens(primary);
  const ontologyGraph = entry.ontologyGraph ?? 'urn:chorus:ontology';
  const subdomainUri = `https://jeffbridwell.com/chorus#${primary}`;

  return {
    primary,
    aliases,
    subdomainUri,
    cardDomainTags,
    cardSequenceTags,
    alertFileTokens,
    ontologyGraph,
  };
}

/**
 * Combined set of domain tag search terms for card-search handlers:
 * primary + aliases. Handler typically builds SPARQL/label filter from this.
 */
export function cardDomainSearchLabels(identity: DomainIdentity): string[] {
  return [identity.primary, ...identity.aliases];
}
