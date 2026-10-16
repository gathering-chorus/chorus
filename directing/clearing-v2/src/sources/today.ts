// @domain: messages
// Today page panels: alerts with resolving cards, Jeff's last session end, cards changed since then.

import {
  TodayAlert,
  TodayCardItem,
  AlertItem,
  AlertsResponse,
  DoorSession,
  DoorSessionsResponse,
  CardRow,
  CardsResponse,
} from './models';
import { Fetched, fetchJson, readJson, withTimeout, errorMessage } from './http';
import { formatBostonTime, ownerBase, capitalize, cardStatus } from './format';

const READ_TIMEOUT_MS = 1500;

// The cards door lists no modified time, so a card's moves come from the spine:
// every board move emits card.item.moved {card_id, title, to, role} (Loki).
// appName="cards" narrows the scan to the cards CLI's stream: ~1.5 s for 3.5 days vs 6.6 s for the whole job.
// A land moves its card to Done through card.accepted, not card.item.moved, so both count.
const MOVES_QUERY = '{job="platform-chorus", appName="cards"} |~ `"event":"card\\.(item\\.moved|accepted)"`';
const MOVES_TIMEOUT_MS = 4000;
const DAY_MS = 24 * 60 * 60 * 1000;

interface MoveEvent { event?: string; card_id?: number | string; title?: string; to?: string; role?: string; timestamp?: string }

function alertItems(json: AlertsResponse | AlertItem[] | null): AlertItem[] {
  if (Array.isArray(json)) return json;
  const data = json?.data;
  if (Array.isArray(data)) return data;
  if (data && Array.isArray(data.alerts)) return data.alerts;
  return [];
}

function parseCardNumber(raw: string | number): number {
  return parseInt(String(raw).replace(/\D/g, ''), 10);
}

function resolvingCardOf(item: AlertItem): number {
  let parsed = NaN;
  if (item.resolvingCard) {
    parsed = typeof item.resolvingCard === 'number' ? item.resolvingCard : parseCardNumber(item.resolvingCard);
  } else if (item.links?.resolvingCard) {
    parsed = parseCardNumber(item.links.resolvingCard);
  }
  return isNaN(parsed) ? 0 : parsed;
}

/** One alert row, or null when the item is neither firing nor acked. `seen` = alerts kept so far. */
function toTodayAlert(item: AlertItem, seen: number): TodayAlert | null {
  const rawStatus = (item.status || item.state || '').toLowerCase();
  if (rawStatus !== 'firing' && rawStatus !== 'acked') return null;
  const status: TodayAlert['status'] = rawStatus;
  const resolvingCard = resolvingCardOf(item);
  const fallbackMeta = resolvingCard > 0 ? `${status === 'firing' ? 'new' : 'changed'} · #${resolvingCard}` : status;
  return {
    id: String(item.id || item.name || `alert-${seen + 1}`),
    status,
    title: item.label || item.title || item.name || '',
    resolvingCard,
    meta: item.meta || fallbackMeta,
  };
}

/** Alerts with resolving cards, read from alertsUrl (zero literals). */
export async function fetchAlerts(alertsUrl: string): Promise<Fetched<TodayAlert[]>> {
  const r = await fetchJson<AlertsResponse | AlertItem[] | null>(alertsUrl, READ_TIMEOUT_MS);
  if (!r.ok) return r;
  const alerts: TodayAlert[] = [];
  for (const item of alertItems(r.value)) {
    const alert = toTodayAlert(item, alerts.length);
    if (alert) alerts.push(alert);
  }
  return { ok: true, value: alerts };
}

function isJeffSession(s: DoorSession): boolean {
  return Boolean(s.ownedBy?.includes('jeff') || s.actsAs?.includes('jeff') || s.name?.startsWith('jeff'));
}

function closedAt(s: DoorSession): Date | null {
  if (!isJeffSession(s) || s.sessionState !== 'closed' || !s.endedAt) return null;
  const d = new Date(s.endedAt);
  return isNaN(d.getTime()) ? null : d;
}

/** When Jeff's most recent closed Clearing session ended; value null when none is on record. */
export async function fetchLastJeffSessionEnd(sessionsUrl: string): Promise<Fetched<Date | null>> {
  const sep = sessionsUrl.includes('?') ? '&' : '?';
  const url = sessionsUrl.includes('limit=') ? sessionsUrl : `${sessionsUrl}${sep}limit=500`;
  const r = await fetchJson<DoorSessionsResponse | null>(url, READ_TIMEOUT_MS);
  if (!r.ok) return r;
  let latest: Date | null = null;
  for (const s of r.value?.data || []) {
    const d = closedAt(s);
    if (d && (!latest || d > latest)) latest = d;
  }
  return { ok: true, value: latest };
}

function changedSince(modifiedStr: string | undefined, since: Date | null): Date | null {
  const modified = modifiedStr ? new Date(modifiedStr) : null;
  // A card MUST have a valid modified timestamp and MUST have been modified strictly after Jeff's last session ended
  if (!modified || isNaN(modified.getTime())) return null;
  if (since && modified <= since) return null;
  return modified;
}

function listOwner(rawAssignee: string | number): string {
  const raw = String(rawAssignee);
  const base = ownerBase(raw);
  return base === 'abby-normal' ? 'Abby Normal' : capitalize(base);
}

function cardRowId(c: CardRow): number {
  const rawId = c.id || c.data?.id || c.name;
  return typeof rawId === 'number' ? rawId : parseInt(String(rawId).split(':').pop() || '0', 10);
}

function toChangedCard(c: CardRow, since: Date | null): TodayCardItem | null {
  const data = c.data || c;
  const modified = changedSince(data.modified || data.created, since);
  if (!modified) return null;
  const id = cardRowId(c);
  if (!id) return null;
  return {
    id,
    title: data.label || data.title || `Card #${id}`,
    status: cardStatus(data.status),
    owner: listOwner(c.links?.assignee || c.links?.ownedBy || data.assignee || data.owner || 'team'),
    time: formatBostonTime(modified.toISOString()),
  };
}

/** Cards changed since `since`; retries without the since filter if the door refuses it. */
async function fetchChangedFromCollection(cardsUrl: string, since: Date | null): Promise<Fetched<TodayCardItem[]>> {
  const sinceParam = since ? `&since=${encodeURIComponent(since.toISOString())}` : '';
  let res: Response;
  try {
    res = await withTimeout(READ_TIMEOUT_MS, async (signal) => {
      const first = await fetch(`${cardsUrl}?limit=50${sinceParam}`, { signal });
      if (!first.ok && sinceParam) return fetch(`${cardsUrl}?limit=50`, { signal });
      return first;
    });
  } catch (err) {
    return { ok: false, message: errorMessage(err) };
  }
  const r = await readJson<CardsResponse | null>(res);
  if (!r.ok) return r;
  const changed: TodayCardItem[] = [];
  for (const c of r.value?.data || []) {
    const item = toChangedCard(c, since);
    if (item) changed.push(item);
  }
  return { ok: true, value: changed };
}

/** Latest move per card since `since` (the last 24 h when there is no watermark). */
async function fetchChangedFromSpine(lokiUrl: string, since: Date | null): Promise<Fetched<TodayCardItem[]>> {
  const from = since ? since.getTime() : Date.now() - DAY_MS;
  const url = `${lokiUrl}/loki/api/v1/query_range?query=${encodeURIComponent(MOVES_QUERY)}`
    + `&start=${from}000000&limit=500&direction=backward`;
  const r = await fetchJson<{ data?: { result?: Array<{ values?: Array<[string, string]> }> } } | null>(url, MOVES_TIMEOUT_MS);
  if (!r.ok) return r;
  const latest = new Map<number, TodayCardItem>();
  const lines = (r.value?.data?.result || []).flatMap((st) => st.values || []);
  lines.sort((a, b) => (a[0] < b[0] ? 1 : -1)); // newest first
  for (const [, line] of lines) {
    const item = toMovedCard(line);
    if (item && !latest.has(item.id)) latest.set(item.id, item);
  }
  return { ok: true, value: [...latest.values()] };
}

function toMovedCard(line: string): TodayCardItem | null {
  let ev: MoveEvent;
  try { ev = JSON.parse(line) as MoveEvent; } catch { return null; } // a non-JSON log line is not a move
  const id = parseCardNumber(ev.card_id ?? '');
  if (!id || !ev.timestamp) return null;
  return {
    id,
    title: ev.title || `Card #${id}`,
    status: cardStatus(ev.event === 'card.accepted' ? 'Done' : ev.to),
    owner: listOwner(ev.role || 'team'),
    time: formatBostonTime(new Date(ev.timestamp).toISOString()),
  };
}

/** What changed since Jeff's last Clearing session ended. */
export interface ChangedCards { items: TodayCardItem[]; unmeasured?: string }

/** What changed since Jeff's last Clearing session ended. When the moves source cannot be
 *  read, the panel says so by name instead of showing an empty list as "nothing changed". */
export async function fetchChangedCards(cardsUrl: string, since: Date | null, lokiUrl?: string): Promise<ChangedCards> {
  const fromCollection = await fetchChangedFromCollection(cardsUrl, since);
  const changed = fromCollection.ok ? fromCollection.value : [];
  if (changed.length > 0 || !lokiUrl) return { items: changed };
  const fromSpine = await fetchChangedFromSpine(lokiUrl, since);
  return fromSpine.ok ? { items: fromSpine.value } : { items: [], unmeasured: `card moves unreadable (${fromSpine.message})` };
}
