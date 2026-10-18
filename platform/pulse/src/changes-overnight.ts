/**
 * changes-overnight.ts — #4563: Cards changed overnight derived once in pulse.
 *
 * Watermark = Jeff's last closed browser session end from Athena sessions door.
 * Changes = card.item.moved and card.accepted events from Loki since watermark.
 * Exposes pure deriveChangesOvernight() plus ChangesOvernightService with 15s cache.
 */

export interface RawSessionRow {
  name: string;
  ownedBy?: string;
  actsAs?: string;
  channel?: string;
  sessionState?: string;
  issuedAt?: string;
  startedAt?: string;
  lastSeenAt?: string;
  endedAt?: string;
}

export interface RawCardMove {
  event?: string;
  card_id?: number | string;
  title?: string;
  to?: string;
  role?: string;
  timestamp?: string;
}

export interface ChangedCard {
  id: number;
  title: string;
  status: string;
  owner: string;
  movedAt: string;
}

export interface ChangesFactInput {
  now?: Date;
  sessions?: RawSessionRow[];
  lokiMoves?: RawCardMove[];
  unmeasured?: Array<{ source: string; reason: string }>;
}

export interface ChangesOvernightResponse {
  derivedAt: string;
  watermark: string | null;
  cards: ChangedCard[];
  unmeasured: Array<{ source: string; reason: string }>;
}

function isJeffSession(s: RawSessionRow): boolean {
  return Boolean(
    s.ownedBy?.includes('jeff') ||
    s.actsAs?.includes('jeff') ||
    s.name.startsWith('jeff')
  );
}

function sessionStartTime(s: RawSessionRow): number {
  const t = s.startedAt || s.issuedAt;
  return t ? new Date(t).getTime() : 0;
}

function sessionEndTime(s: RawSessionRow): { ms: number; iso: string } | null {
  const endIso = s.lastSeenAt || s.endedAt;
  if (!endIso) return null;
  const ms = new Date(endIso).getTime();
  return isNaN(ms) ? null : { ms, iso: endIso };
}

function findPriorSessionEnd(jeffSessions: RawSessionRow[]): string | null {
  let liveSession = jeffSessions[0];
  let liveStartMs = sessionStartTime(liveSession);

  for (const s of jeffSessions.slice(1)) {
    const startMs = sessionStartTime(s);
    if (startMs > liveStartMs) {
      liveSession = s;
      liveStartMs = startMs;
    }
  }

  const prior = jeffSessions.filter((s) => s !== liveSession && sessionStartTime(s) < liveStartMs);
  let latestPriorMs = 0;
  let latestPriorIso: string | null = null;

  for (const s of prior) {
    const end = sessionEndTime(s);
    if (end && end.ms > latestPriorMs) {
      latestPriorMs = end.ms;
      latestPriorIso = end.iso;
    }
  }

  return latestPriorIso;
}

export function findJeffLastBrowserSessionEnd(sessions?: RawSessionRow[]): string | null {
  if (!sessions) return null;
  const jeffSessions = sessions.filter((s) => s.channel === 'browser' && isJeffSession(s));
  if (jeffSessions.length === 0) return null;

  if (jeffSessions.length > 1) {
    const priorEnd = findPriorSessionEnd(jeffSessions);
    if (priorEnd) return priorEnd;
  }

  let latestMs = 0;
  let latestIso: string | null = null;
  for (const s of jeffSessions) {
    const end = sessionEndTime(s);
    if (end && end.ms > latestMs) {
      latestMs = end.ms;
      latestIso = end.iso;
    }
  }

  return latestIso;
}

function normalizeOwner(role?: string): string {
  if (!role) return 'Team';
  const clean = role.toLowerCase().replace(/^principal-/, '');
  if (clean === 'abby-normal') return 'Abby Normal';
  return clean.charAt(0).toUpperCase() + clean.slice(1);
}

function parseCardId(cardId?: number | string): number {
  if (typeof cardId === 'number') return cardId;
  const parsed = parseInt(String(cardId || '').replace(/\D/g, ''), 10);
  return isNaN(parsed) ? 0 : parsed;
}

function toChangedCard(m: RawCardMove): ChangedCard | null {
  const id = parseCardId(m.card_id);
  if (!id || !m.timestamp) return null;
  const status = m.event === 'card.accepted' ? 'Done' : (m.to || 'Done');
  return {
    id,
    title: m.title || `Card #${id}`,
    status,
    owner: normalizeOwner(m.role),
    movedAt: m.timestamp,
  };
}

function collectNewestCardMoves(moves: RawCardMove[], watermarkMs: number): ChangedCard[] {
  const latestByCard = new Map<number, { card: ChangedCard; ms: number }>();

  for (const m of moves) {
    if (!m.timestamp) continue;
    const ms = new Date(m.timestamp).getTime();
    if (isNaN(ms) || ms <= watermarkMs) continue;
    const item = toChangedCard(m);
    if (!item) continue;

    const existing = latestByCard.get(item.id);
    if (!existing || ms > existing.ms) {
      latestByCard.set(item.id, { card: item, ms });
    }
  }

  const result = Array.from(latestByCard.values()).map((v) => v.card);
  result.sort((a, b) => new Date(b.movedAt).getTime() - new Date(a.movedAt).getTime());
  return result;
}

export function deriveChangesOvernight(input: ChangesFactInput): ChangesOvernightResponse {
  const derivedAt = input.now ? input.now.toISOString() : new Date().toISOString();
  const unmeasured = input.unmeasured ?? [];

  const sessionsDown = unmeasured.some((u) => u.source.includes('session'));
  if (sessionsDown || !input.sessions) {
    const reasons = unmeasured.filter((u) => u.source.includes('session'));
    const unmeasuredOut = reasons.length > 0 ? unmeasured : [
      { source: 'athena:identity:sessions', reason: 'sessions door unreadable' },
      ...unmeasured,
    ];
    return { derivedAt, watermark: null, cards: [], unmeasured: unmeasuredOut };
  }

  const watermark = findJeffLastBrowserSessionEnd(input.sessions);
  if (!watermark) {
    return {
      derivedAt,
      watermark: null,
      cards: [],
      unmeasured: [{ source: 'athena:identity:sessions', reason: 'no closed browser session on record' }],
    };
  }

  const lokiDown = unmeasured.some((u) => u.source.includes('loki') || u.source.includes('move'));
  if (lokiDown || !input.lokiMoves) {
    const reasons = unmeasured.filter((u) => u.source.includes('loki') || u.source.includes('move'));
    const unmeasuredOut = reasons.length > 0 ? unmeasured : [
      { source: 'loki:moves', reason: 'card moves unreadable' },
      ...unmeasured,
    ];
    return { derivedAt, watermark, cards: [], unmeasured: unmeasuredOut };
  }

  const watermarkMs = new Date(watermark).getTime();
  const cards = collectNewestCardMoves(input.lokiMoves, watermarkMs);

  return {
    derivedAt,
    watermark,
    cards,
    unmeasured,
  };
}

export class ChangesOvernightService {
  private cache: ChangesOvernightResponse = {
    derivedAt: new Date().toISOString(),
    watermark: null,
    cards: [],
    unmeasured: [],
  };
  private timer?: NodeJS.Timeout;
  private readonly pollIntervalMs: number;
  private readonly factFetcher: () => Promise<ChangesFactInput>;

  constructor(factFetcher: () => Promise<ChangesFactInput>, pollIntervalMs = 15000) {
    this.factFetcher = factFetcher;
    this.pollIntervalMs = pollIntervalMs;
  }

  async refresh(): Promise<ChangesOvernightResponse> {
    try {
      const input = await this.factFetcher();
      this.cache = deriveChangesOvernight(input);
      return this.cache;
    } catch (err: unknown) {
      const reason = err instanceof Error ? err.message : String(err);
      this.cache = {
        derivedAt: new Date().toISOString(),
        watermark: null,
        cards: [],
        unmeasured: [{ source: 'pulse:changes-overnight', reason }],
      };
      return this.cache;
    }
  }

  getChanges(): ChangesOvernightResponse {
    return this.cache;
  }

  start(): void {
    if (this.timer) return;
    void this.refresh();
    this.timer = setInterval(() => {
      void this.refresh();
    }, this.pollIntervalMs);
    this.timer.unref();
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }
}

const MOVES_QUERY = '{job="platform-chorus", appName="cards"} |~ `"event":"card\\.(item\\.moved|accepted)"`';

async function fetchLiveSessions(
  athenaBase: string,
  unmeasured: Array<{ source: string; reason: string }>
): Promise<RawSessionRow[] | undefined> {
  try {
    const res = await fetch(`${athenaBase}/v1/identity/sessions?limit=500`, { signal: AbortSignal.timeout(2000) });
    if (!res.ok) {
      unmeasured.push({ source: 'athena:identity:sessions', reason: `HTTP ${res.status}` });
      return undefined;
    }
    const json = (await res.json()) as { data?: RawSessionRow[] };
    return json.data || [];
  } catch (e: unknown) {
    unmeasured.push({ source: 'athena:identity:sessions', reason: e instanceof Error ? e.message : String(e) });
    return undefined;
  }
}

function parseLokiMoveLines(lines: Array<[string, string]>): RawCardMove[] {
  const result: RawCardMove[] = [];
  for (const [, line] of lines) {
    try {
      result.push(JSON.parse(line) as RawCardMove);
    } catch {
      // Ignore non-json lines
    }
  }
  return result;
}

/** #4623 — each tick re-reads this much before the last read's end, for
 *  lines promtail ships late. Duplicates in the overlap are dropped by key. */
export const LOKI_REREAD_OVERLAP_MS = 2 * 60_000;

/** #4623 — what one fetcher has already read since the current watermark. */
interface MovesCache {
  watermark: string;
  readEndMs: number;
  lines: Map<string, RawCardMove>;
}

async function fetchLiveLokiMoves(
  lokiBase: string,
  watermark: string,
  cache: { current?: MovesCache },
  unmeasured: Array<{ source: string; reason: string }>
): Promise<RawCardMove[] | undefined> {
  if (lokiBase === 'off') {
    unmeasured.push({ source: 'loki:moves', reason: 'loki off in this environment' });
    return undefined;
  }
  try {
    const watermarkMs = new Date(watermark).getTime();
    // #4623 — a full read from the watermark only when it changed; otherwise
    // only what's new since the last read. Measured 2026-10-18: the full read
    // every 15s split into ~150 internal Loki queries per tick.
    const prior = cache.current?.watermark === watermark ? cache.current : undefined;
    const startMs = prior ? Math.max(watermarkMs, prior.readEndMs - LOKI_REREAD_OVERLAP_MS) : watermarkMs;
    const endMs = Date.now();
    const url = `${lokiBase}/loki/api/v1/query_range?query=${encodeURIComponent(MOVES_QUERY)}` +
      `&start=${startMs}000000&end=${endMs}000000&limit=500&direction=backward`;
    const res = await fetch(url, { signal: AbortSignal.timeout(3000) });
    if (!res.ok) {
      unmeasured.push({ source: 'loki:moves', reason: `HTTP ${res.status}` });
      return undefined;
    }
    const json = (await res.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
    const values = (json.data?.result || []).flatMap((st) => st.values || []);
    const lines = new Map(prior ? prior.lines : []);
    for (const [ts, line] of values) {
      const [move] = parseLokiMoveLines([[ts, line]]);
      if (move) lines.set(`${ts}|${line}`, move);
    }
    cache.current = { watermark, readEndMs: endMs, lines };
    return [...lines.values()];
  } catch (e: unknown) {
    unmeasured.push({ source: 'loki:moves', reason: e instanceof Error ? e.message : String(e) });
    return undefined;
  }
}

export function buildLiveChangesOvernightFetcher(
  athenaBase = process.env.ATHENA_URL || 'http://localhost:3360',
  lokiBase = process.env.LOKI_URL || 'http://localhost:3102'
): () => Promise<ChangesFactInput> {
  const cache: { current?: MovesCache } = {};
  return async (): Promise<ChangesFactInput> => {
    const unmeasured: Array<{ source: string; reason: string }> = [];
    const sessions = await fetchLiveSessions(athenaBase, unmeasured);
    const watermark = findJeffLastBrowserSessionEnd(sessions);

    if (!watermark) {
      return { now: new Date(), sessions, lokiMoves: [], unmeasured };
    }

    const lokiMoves = await fetchLiveLokiMoves(lokiBase, watermark, cache, unmeasured);
    return { now: new Date(), sessions, lokiMoves, unmeasured };
  };
}
