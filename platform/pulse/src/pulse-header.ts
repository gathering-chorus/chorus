/**
 * pulse-header.ts — #4757: GET /pulse, the header every role's turn shows.
 *
 * Jeff 2026-10-26: "why are we writing prompts we already have messages api". Each line is
 * READ, at request time, from the API that already holds it — card steps from /cws, roles
 * from pulse's actor-state, messages from pulse's own store, alerts from the alerts stream.
 * Pulse stores nothing for the header. A source that cannot be read says so on its line.
 */
import type { Express } from 'express';
import { readCardSteps, type CwsRow } from './card-steps';
import { factsFromActorState, QUIET_AFTER_S } from './pulse-tree';

export interface Line { source: string; rows?: Array<Record<string, unknown>>; unmeasured?: string }
export interface Header { kind: 'Header'; product: 'pulse'; at: string; data: Record<string, Line> }

type Read<T> = () => Promise<T>;
export interface Sources {
  cards: Read<CwsRow[]>;
  actorState: () => unknown;
  messages: () => Array<{ from: string; content: string; created_at?: string; createdAt?: string }>;
  alerts: Read<Array<{ name: string; label?: string }>>;
}

const why = (e: unknown) => (e instanceof Error ? e.message : String(e)).split('\n')[0].slice(0, 160);

async function line<T>(source: string, read: Read<T>, rows: (v: T) => Array<Record<string, unknown>>): Promise<Line> {
  try {
    return { source, rows: rows(await read()) };
  } catch (e) {
    return { source, unmeasured: why(e) };
  }
}

/** The header, each line read from its source now. */
export async function composeHeader(src: Sources, now = new Date()): Promise<Header> {
  const [cards, roles, messages, alerts] = await Promise.all([
    line('/cws', src.cards, wip => wip.map(c => ({
      card: c.card, role: c.role, step: c.step,
      waitedSeconds: c.live && c.since ? Math.max(0, Math.round((now.getTime() - Date.parse(c.since)) / 1000)) : null,
    }))),
    line('/api/actor-state', () => Promise.resolve(src.actorState()), states => {
      const { actors, lastCalls } = factsFromActorState(states);
      if (!actors.length) throw new Error('actor-state lists no actors');
      const calls = new Map(Object.entries(lastCalls));
      return actors.map(role => {
        const last = calls.get(role);
        const s = last ? Math.max(0, Math.round((now.getTime() - Date.parse(last)) / 1000)) : null;
        return { role, state: s !== null && s < QUIET_AFTER_S ? 'moving' : 'quiet', secondsSinceLastCall: s };
      });
    }),
    line('/api/messages', () => Promise.resolve(src.messages()), ms => ms.map(m => ({
      from: m.from, sentAt: m.created_at ?? m.createdAt ?? '', body: m.content,
    }))),
    line('/pulse/alerts/alerts', src.alerts, as => as.map(a => ({ name: a.name, label: a.label ?? '' }))),
  ]);
  return { kind: 'Header', product: 'pulse', at: now.toISOString(), data: { cards, roles, messages, alerts } };
}

/** /cws read at most every 30 s: each read asks dagu about every WIP card. */
export function cachedCards(read: Read<CwsRow[]> = readCardSteps, ttlMs = 30_000): Read<CwsRow[]> {
  let last: { at: number; rows: CwsRow[] } | undefined;
  return async () => {
    if (last && Date.now() - last.at < ttlMs) return last.rows;
    const rows = await read();
    last = { at: Date.now(), rows };
    return rows;
  };
}

export async function readAlerts(base = process.env.PULSE_API_URL || 'http://127.0.0.1:3392'): Promise<Array<{ name: string; label?: string }>> {
  const res = await fetch(`${base}/pulse/alerts/alerts?limit=50`, { signal: AbortSignal.timeout(4000) });
  if (!res.ok) throw new Error(`alerts stream: HTTP ${res.status}`);
  const body = await res.json() as { data?: Array<{ name?: unknown; label?: unknown }> };
  return (body.data ?? []).map(a => ({ name: typeof a.name === 'string' ? a.name : '', label: typeof a.label === 'string' ? a.label : '' }));
}

export function registerHeaderRoute(app: Express, src: Sources): void {
  app.get('/pulse', (_req, res) => {
    void composeHeader(src).then(h => res.json(h));
  });
}
