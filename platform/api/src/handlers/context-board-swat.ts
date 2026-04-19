/**
 * GET /api/chorus/context/board/swat (#2261).
 *
 * Answers: "What cards are in the SWAT lane right now?"
 * Mirrors context-board-wip.ts but reads board.swat_cards from pulse.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextBoardSwatDeps {
  sparql: StampSparqlClient;
  readPulse: () => string | null;
  now?: () => Date;
}

export interface SwatCard {
  id: number;
  owner: string;
  title: string;
  priority: string;
  domain?: string;
}

export interface ContextBoardSwatResponse {
  status: number;
  body: ContextEnvelope<{ total: number; cards: SwatCard[] }> | { error: string };
}

export async function fetchContextBoardSwat(
  deps: ContextBoardSwatDeps,
  sourceUrl: string,
  roleFilter?: string,
): Promise<ContextBoardSwatResponse> {
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

  const header = await stampHeader(deps.sparql, 'chorus');

  const swatRaw = (pulse as { board?: { swat_cards?: unknown } })?.board?.swat_cards;
  const source = Array.isArray(swatRaw) ? swatRaw : [];

  const all: SwatCard[] = source
    .filter((c): c is Record<string, unknown> => !!c && typeof c === 'object')
    .map((c) => ({
      id: typeof c['id'] === 'number' ? c['id'] : 0,
      owner: typeof c['owner'] === 'string' ? c['owner'] : '',
      title: typeof c['title'] === 'string' ? c['title'] : '',
      priority: typeof c['priority'] === 'string' ? c['priority'] : '',
      ...(typeof c['domain'] === 'string' && { domain: c['domain'] }),
    }));

  const filtered = roleFilter
    ? all.filter((c) => c.owner.toLowerCase() === roleFilter.toLowerCase())
    : all;

  filtered.sort((a, b) => a.id - b.id);

  const envelope = buildEnvelope(header, sourceUrl, { total: filtered.length, cards: filtered });
  return { status: 200, body: envelope };
}
