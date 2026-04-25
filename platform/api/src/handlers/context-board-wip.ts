/* eslint-disable security/detect-object-injection -- Indexing on validated role/status keys. */
/**
 * GET /api/chorus/context/board/wip (#2234 Step 3).
 *
 * Answers: "What cards are in WIP right now?" Optional `role` query param
 * scopes to a single role.
 *
 * Source today: `/tmp/pulse-latest.json`'s `board.wip_cards` — mirrored from
 * the Vikunja board API by the pulse daemon (#1881). Proof-of-shape uses
 * this mirror; a later card can swap DI to a direct board-API call without
 * changing the envelope contract.
 *
 * DI surface: `readPulse` returns the snapshot file contents (or null).
 * `sparql` stamps the envelope header. Tests inject both as stubs.
 *
 * Scope: domain (chorus) — the Context API itself lives in the chorus
 * product. `subdomain` is not set for this endpoint; a future per-domain
 * variant might add it.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextBoardWipDeps {
  sparql: StampSparqlClient;
  /** Returns the raw pulse-latest.json file contents, or null if missing. */
  readPulse: () => string | null;
  now?: () => Date;
}

export interface WipCard {
  id: number;
  owner: string;
  title: string;
  priority: string;
  domain?: string;
  step?: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface ContextBoardWipResponse {
  status: number;
  body: ContextEnvelope<{ total: number; cards: WipCard[] }> | { error: string };
}

export async function fetchContextBoardWip(
  deps: ContextBoardWipDeps,
  sourceUrl: string,
  roleFilter?: string,
): Promise<ContextBoardWipResponse> {
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

  const wipRaw = (pulse as { board?: { wip_cards?: unknown } }).board?.wip_cards;
  const source = Array.isArray(wipRaw) ? wipRaw : [];

  const all: WipCard[] = source
    .filter((c): c is Record<string, unknown> => !!c && typeof c === 'object')
    .map((c) => shapeCard(c));

  const filtered = roleFilter
    ? all.filter((c) => c.owner.toLowerCase() === roleFilter.toLowerCase())
    : all;

  filtered.sort((a, b) => a.id - b.id);

  const envelope = buildEnvelope(header, sourceUrl, {
    total: filtered.length,
    cards: filtered,
  });
  return { status: 200, body: envelope };
}

function shapeCard(raw: Record<string, unknown>): WipCard {
  const str = (k: string): string | undefined => {
    const v = raw[k];
    return typeof v === 'string' ? v : undefined;
  };
  const num = (k: string): number => {
    const v = raw[k];
    return typeof v === 'number' ? v : 0;
  };
  return {
    id: num('id'),
    owner: str('owner') ?? '',
    title: str('title') ?? '',
    priority: str('priority') ?? '',
    ...(str('domain') && { domain: str('domain') }),
    ...(str('valueStream') && { valueStream: str('valueStream') }),
    ...(str('step') && { step: str('step') }),
    ...(str('createdAt') && { createdAt: str('createdAt') }),
    ...(str('updatedAt') && { updatedAt: str('updatedAt') }),
  };
}
