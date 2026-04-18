/**
 * GET /api/athena/card/:id — Card detail with parsed AC items (#2187).
 *
 * Shells out to the cards CLI (JSON mode), then parses AC checkbox items
 * from the card description and attaches them as ac_items.
 *
 * Any shell failure (missing card, CLI missing, JSON parse error) is
 * surfaced as 404 — preserving pre-extraction behavior.
 */
import type { FetchResult } from './codebase-topology';

export interface RawCard {
  description?: string;
  ac_items?: Array<{ text: string; checked: boolean }>;
  [key: string]: unknown;
}

export interface AthenaCardDetailDeps {
  runCardsView: (cardId: string) => Promise<string>;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function parseAcItems(description: string | undefined): Array<{ text: string; checked: boolean }> {
  if (!description) return [];
  const matches = description.match(/- \[([ x])\] .+/g) ?? [];
  return matches.map((line) => ({
    text: line.replace(/^- \[[ x]\] /, ''),
    checked: line.startsWith('- [x]'),
  }));
}

export async function fetchAthenaCardDetail(
  deps: AthenaCardDetailDeps,
  cardId: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const stdout = await deps.runCardsView(cardId);
    const card = JSON.parse(stdout) as RawCard;
    card.ac_items = parseAcItems(card.description);
    return {
      status: 200,
      body: envelope('card-detail', card, now() - start),
    };
  } catch {
    return {
      status: 404,
      body: envelope('card-detail', { error: `Card ${cardId} not found` }, now() - start, { error: true }),
    };
  }
}
