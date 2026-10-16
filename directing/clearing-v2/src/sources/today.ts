// @domain: messages
// Today page: alerts with resolving cards. Who is working and what changed overnight come from pulse (sources/pulse.ts).

import { TodayAlert, AlertItem, AlertsResponse } from './models';
import { Fetched, fetchJson } from './http';

const READ_TIMEOUT_MS = 1500;

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
