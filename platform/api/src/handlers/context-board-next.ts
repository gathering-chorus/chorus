/* eslint-disable security/detect-object-injection -- Indexing on validated role/status keys. */
/**
 * GET /api/chorus/context/board/next?role={r} (#2252).
 *
 * Answers: "What cards are queued next?" Optional `role` scopes to one role.
 *
 * Source: /tmp/pulse-latest.json's `board.next_cards` — added to pulse
 * daemon alongside wip+swat in #2252 so the same cache-primary read path
 * serves all three board surfaces.
 *
 * Scope: domain (chorus).
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextBoardNextDeps {
  sparql: StampSparqlClient;
  readPulse: () => string | null;
}

export interface NextCard {
  id: number;
  owner: string;
  title: string;
  priority: string;
  domain?: string;
  status?: string;
}

export interface ContextBoardNextResponse {
  status: number;
  body: ContextEnvelope<{ total: number; cards: NextCard[] }> | { error: string };
}

export async function fetchContextBoardNext(
  deps: ContextBoardNextDeps,
  sourceUrl: string,
  roleFilter?: string,
): Promise<ContextBoardNextResponse> {
  const raw = deps.readPulse();
  if (raw === null) {
    return { status: 503, body: { error: 'No pulse snapshot available; board state unknown.' } };
  }
  let pulse: unknown;
  try {
    pulse = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `pulse-latest.json unparseable: ${message}` } };
  }

  const rawList = (pulse as { board?: { next_cards?: unknown } }).board?.next_cards;
  const source = Array.isArray(rawList) ? rawList : [];
  const all: NextCard[] = source
    .filter((c): c is Record<string, unknown> => !!c && typeof c === 'object')
    .map((c) => shapeCard(c));

  const filtered = roleFilter
    ? all.filter((c) => c.owner.toLowerCase() === roleFilter.toLowerCase())
    : all;
  filtered.sort((a, b) => a.id - b.id);

  const header = await stampHeader(deps.sparql, 'chorus');
  return {
    status: 200,
    body: buildEnvelope(header, sourceUrl, { total: filtered.length, cards: filtered }),
  };
}

function shapeCard(raw: Record<string, unknown>): NextCard {
  const str = (k: string): string | undefined => (typeof raw[k] === 'string' ? (raw[k] as string) : undefined);
  const num = (k: string): number => (typeof raw[k] === 'number' ? (raw[k] as number) : 0);
  const card: NextCard = {
    id: num('id'),
    owner: str('owner') ?? '',
    title: str('title') ?? '',
    priority: str('priority') ?? '',
  };
  const domain = str('domain');
  if (domain) card.domain = domain;
  const status = str('status');
  if (status) card.status = status;
  return card;
}
