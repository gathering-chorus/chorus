/**
 * GET /api/chorus/context/board/wip (#2234 Step 3).
 *
 * Answers: "What cards are in WIP right now?" Optional `role` query param
 * scopes to a single role.
 *
 * #4987 — Source: the board itself, its own database read at the moment of the question
 * (server.ts readWipFromBoard). It used to answer from pulse-latest.json's board.wip_cards, a
 * copy of a copy (`cards list` → /tmp/board-wip-snapshot.json → pulse-latest.json) that
 * served its last list at any age. On 2026-11-17 that copy kept #4986 in WIP after its
 * land: the pulse header said "reopened" and the Teammates tile said "building #4986"
 * while the board said Done. A board read that is missing or older than
 * BOARD_MAX_AGE_MS says unreadable, never an old list shown as now.
 *
 * DI surface: `readBoard` returns the cached cards and when they were last read.
 * `sparql` stamps the envelope header. Tests inject both as stubs.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

/** A board read older than this is too old to call "now" (the cache re-reads every 60 s). */
export const BOARD_MAX_AGE_MS = 5 * 60 * 1000;

export interface BoardCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  priority: string;
  domain: string;
}

/** #4987 — one board row (task, bucket, labels joined with |) as a card: the owner,
 *  priority and domain are the board's own labels (owner:wren, P2, domain:chorus). */
export function boardCardFromRow(row: { id: number; title: string; status: string; labels: string }): BoardCard {
  const labels = row.labels ? row.labels.split('|') : [];
  const value = (prefix: string) => (labels.find((l) => l.startsWith(prefix)) ?? '').slice(prefix.length);
  return {
    id: String(row.id),
    title: row.title,
    status: row.status,
    owner: value('owner:').toLowerCase(),
    priority: labels.find((l) => /^P[1-3]$/.test(l)) ?? '',
    domain: value('domain:'),
  };
}

export interface ContextBoardWipDeps {
  sparql: StampSparqlClient;
  /** The board's cards and when they were read (ms since epoch; 0 = never). Throws when
   *  the board cannot be read. */
  readBoard: () => { cards: BoardCard[]; readAt: number };
  now?: () => Date;
}

export interface WipCard {
  id: number;
  owner: string;
  title: string;
  priority: string;
  domain?: string;
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
  let read: { cards: BoardCard[]; readAt: number };
  try {
    read = deps.readBoard();
  } catch (e) {
    return { status: 503, body: { error: `board unreadable: ${e instanceof Error ? e.message : String(e)}` } };
  }
  const { cards, readAt } = read;
  const now = (deps.now ?? (() => new Date()))().getTime();
  if (readAt === 0) {
    return { status: 503, body: { error: 'board unreadable: not read yet' } };
  }
  if (now - readAt > BOARD_MAX_AGE_MS) {
    return { status: 503, body: { error: `board unreadable: last read ${Math.round((now - readAt) / 1000)} s ago` } };
  }

  const header = await stampHeader(deps.sparql, 'chorus');
  const all: WipCard[] = cards.filter((c) => c.status === 'WIP').map(shapeCard).filter((c) => Number.isFinite(c.id));
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

function shapeCard(c: BoardCard): WipCard {
  return {
    id: Number(c.id),
    owner: c.owner,
    title: c.title,
    priority: c.priority,
    ...(c.domain && { domain: c.domain }),
  };
}
