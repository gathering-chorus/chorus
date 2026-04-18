/**
 * GET /api/athena/subdomains/:id/cards — Board cards related to this domain (#2187).
 *
 * Maps the sub-domain id to a search label (stripping -domain/-service/-analytics),
 * expands through DOMAIN_ALIASES for aggregation domains, filters the board
 * cards cache for any matching domain:<label> or sequence:<label> tag.
 */
import type { FetchResult } from './codebase-topology';

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

const DOMAIN_ALIASES: Record<string, string[]> = {
  tests: ['quality'],
  code: ['code'],
  gates: ['gates'],
};

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

export async function fetchAthenaSubdomainCards(
  deps: AthenaSubdomainCardsDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const domainLabel = id.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
    const searchLabels = [domainLabel, ...(DOMAIN_ALIASES[domainLabel] ?? [])];
    const cards = deps.getBoardCards()
      .filter((c) => searchLabels.some((l) => c.tags.includes(`domain:${l}`) || c.tags.includes(`sequence:${l}`)))
      .map((c) => ({ id: c.id, title: c.title, owner: c.owner, status: c.status, priority: c.priority }));
    // Note: tags is a comma-joined string (CachedCard shape), not an array.
    // .includes() does substring match here; semantics match pre-extraction.
    return {
      status: 200,
      body: envelope(
        'subdomain-cards',
        { subdomain: id, domainLabel, cards },
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
