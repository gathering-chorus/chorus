/**
 * pulse-header.ts — #4757: GET /pulse, the header every role's turn shows.
 *
 * Jeff 2026-10-26: "why are we writing prompts we already have messages api". Each line is
 * READ, at request time, from the API that already holds it — card steps from /cws, roles
 * from pulse's actor-state, messages and alerts from the generated pulse API (#4761: one path).
 * Pulse stores nothing for the header. A source that cannot be read says so on its line.
 */
import type { Express } from 'express';
import { readCardSteps, type CwsRow } from './card-steps';
import { factsFromActorState, QUIET_AFTER_S } from './pulse-tree';

export interface Line { source: string; rows?: Array<Record<string, unknown>>; unmeasured?: string; staleSeconds?: number }
export interface Header { kind: 'Header'; product: 'pulse'; at: string; data: Record<string, Line> }

type Read<T> = () => Promise<T>;
export interface Sources {
  cards: Read<CwsRow[]> & { staleSeconds?: () => number };
  actorState: () => unknown;
  messages: Read<Array<{ senderName: string; messageBody: string; sentAt: string }>>;
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
    line('/pulse/messages/messages', src.messages, ms => ms.map(m => ({
      from: m.senderName, sentAt: m.sentAt, body: m.messageBody,
    }))),
    line('/pulse/alerts/alerts', src.alerts, as => as.map(a => ({ name: a.name, label: a.label ?? '' }))),
  ]);
  // #4807 — cards served from the last read while refreshes fail say how old they are
  const stale = src.cards.staleSeconds?.() ?? 0;
  if (stale > 0 && cards.rows) cards.staleSeconds = stale;
  return { kind: 'Header', product: 'pulse', at: now.toISOString(), data: { cards, roles, messages, alerts } };
}

/**
 * /cws read at most every 30 s: each read asks dagu about every WIP card.
 * #4807 — one read at a time, shared by every caller (before, each /pulse past the TTL
 * started its own ~2 s script and they piled up until /pulse stopped answering, and the
 * prompt hook waiting on /pulse lost the prompt's context). Once a read has landed,
 * callers get the last cards at once while a refresh runs. A read that fails, or passes
 * timeoutMs (its child is killed through the signal), is dropped and the next caller
 * starts a new one. With nothing read yet a caller waits at most coldWaitMs, inside the
 * prompt hook's 5 s budget, and the line says unmeasured while the read carries on.
 */
export function cachedCards(
  read: (signal?: AbortSignal) => Promise<CwsRow[]> = signal => readCardSteps(undefined, signal),
  ttlMs = 30_000, timeoutMs = 10_000, coldWaitMs = 2_000,
): Read<CwsRow[]> & { staleSeconds: () => number } {
  let last: { at: number; rows: CwsRow[] } | undefined;
  let inflight: Promise<CwsRow[]> | undefined;
  const refresh = (): Promise<CwsRow[]> => {
    if (inflight) return inflight;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timedOut = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        abort.abort();
        reject(new Error(`/cws read: no answer in ${timeoutMs} ms`));
      }, timeoutMs);
    });
    const p = Promise.race([read(abort.signal), timedOut])
      .then(rows => { last = { at: Date.now(), rows }; return rows; })
      .finally(() => { clearTimeout(timer); if (inflight === p) inflight = undefined; });
    inflight = p;
    p.catch(() => undefined);
    return p;
  };
  const staleSeconds = () => (last && Date.now() - last.at >= ttlMs ? Math.round((Date.now() - last.at) / 1000) : 0);
  return Object.assign(async () => {
    if (last && Date.now() - last.at < ttlMs) return last.rows;
    if (last) { void refresh(); return last.rows; }
    let wait: ReturnType<typeof setTimeout> | undefined;
    const cold = new Promise<never>((_, reject) => {
      wait = setTimeout(() => reject(new Error(`/cws refreshing: first read not back in ${coldWaitMs} ms`)), coldWaitMs);
    });
    try { return await Promise.race([refresh(), cold]); } finally { clearTimeout(wait); }
  }, { staleSeconds });
}

export async function readAlerts(base = process.env.PULSE_API_URL || 'http://127.0.0.1:3392'): Promise<Array<{ name: string; label?: string }>> {
  const res = await fetch(`${base}/pulse/alerts/alerts?limit=50`, { signal: AbortSignal.timeout(4000) });
  if (!res.ok) throw new Error(`alerts stream: HTTP ${res.status}`);
  const body = await res.json() as { data?: Array<{ name?: unknown; label?: unknown }> };
  return (body.data ?? []).map(a => ({ name: typeof a.name === 'string' ? a.name : '', label: typeof a.label === 'string' ? a.label : '' }));
}

/** #4761 — the newest messages, from the generated route (the hand-written /api/messages is retired). */
export async function readMessages(base = process.env.PULSE_API_URL || 'http://127.0.0.1:3392'): Promise<Array<{ senderName: string; messageBody: string; sentAt: string }>> {
  const res = await fetch(`${base}/pulse/messages/messages?sort=-sentAt&limit=5`, { signal: AbortSignal.timeout(4000) });
  if (!res.ok) throw new Error(`messages: HTTP ${res.status}`);
  const body = await res.json() as { data?: Array<{ senderName?: unknown; messageBody?: unknown; sentAt?: unknown }> };
  const str = (v: unknown) => (typeof v === 'string' ? v : '');
  return (body.data ?? []).map(m => ({ senderName: str(m.senderName), messageBody: str(m.messageBody), sentAt: str(m.sentAt) }));
}

export function registerHeaderRoute(app: Express, src: Sources): void {
  app.get('/pulse', (_req, res) => {
    void composeHeader(src).then(h => res.json(h));
  });
}
