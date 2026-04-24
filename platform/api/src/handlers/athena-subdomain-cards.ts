/**
 * GET /api/athena/subdomains/:id/cards — Board cards related to this domain (#2187).
 *
 * Uses the shared domain-identity resolver (#2430) so sub-subdomain aliases
 * live in one registry. Replaces the handler-local DOMAIN_ALIASES table and
 * the over-aggressive -analytics/-service suffix strip that used to collapse
 * `loom-analytics` to `loom` by accident.
 */
import type { FetchResult } from './codebase-topology';
import { resolveDomainIdentity, cardDomainSearchLabels } from './domain-identity';

/**
 * Shape matches server.ts CachedCard — id is a string (card ID rendered),
 * tags is a comma-joined string (the inline code used .includes() as a
 * substring match, not array containment). Preserved verbatim.
 */
export interface BoardCard {
  id: string;
  title: string;
  owner: string;
  status: string;
  priority: string;
  tags: string;
}

export interface AthenaSubdomainCardsDeps {
  getBoardCards: () => BoardCard[];
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

export function fetchAthenaSubdomainCards(
  deps: AthenaSubdomainCardsDeps,
  id: string,
): FetchResult {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const identity = resolveDomainIdentity(id);
    const searchLabels = cardDomainSearchLabels(identity);
    const cards = deps.getBoardCards()
      .filter((c) => searchLabels.some((l) => c.tags.includes(`domain:${l}`) || c.tags.includes(`sequence:${l}`)))
      .map((c) => ({ id: c.id, title: c.title, owner: c.owner, status: c.status, priority: c.priority }));
    // Note: tags is a comma-joined string (CachedCard shape), not an array.
    // .includes() does substring match here; semantics match pre-extraction.
    return {
      status: 200,
      body: envelope(
        'subdomain-cards',
        { subdomain: id, domainLabel: identity.primary, cards },
        now() - start,
        { count: cards.length },
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-cards', { error: message }, now() - start, { error: true }),
    };
  }
}
