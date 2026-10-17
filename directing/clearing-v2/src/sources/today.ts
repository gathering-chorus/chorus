// @domain: messages
// Today page: alerts from the alerts API (trigger event + trigger data, #4574). Who is working and what changed overnight come from pulse (sources/pulse.ts).

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

function alertStatusOf(item: AlertItem): TodayAlert['status'] | null {
  const raw = (item.status || item.state || '').toLowerCase();
  if (raw && raw !== 'firing' && raw !== 'acked') return null;
  return raw === 'acked' ? 'acked' : 'firing';
}

function alertTitleOf(item: AlertItem): string {
  // never item.description: that is the alert's 'why' (Jeff 10-16: trigger + data, no why)
  return item.trigger || item.label || item.title || item.name || '';
}

function alertMetaOf(item: AlertItem, title: string, status: TodayAlert['status'], resolvingCard: number): string {
  const card = resolvingCard > 0 ? ` · #${resolvingCard}` : '';
  // trigger data is the measured values the API serves; until it serves them, say so
  if (item.data) return `${item.data}${card}`;
  return `no trigger data yet${card}`;
}

/** #4574 — trigger event first (rule/name), then trigger data (measured values), never the description.
 *  Rows from /api/chorus/context/alerts omit status because data.alerts lists firing rows only. */
function toTodayAlert(item: AlertItem, seen: number): TodayAlert | null {
  const status = alertStatusOf(item);
  if (!status) return null;
  const resolvingCard = resolvingCardOf(item);
  const title = alertTitleOf(item);
  return {
    id: String(item.id || item.name || `alert-${seen + 1}`),
    status,
    title,
    resolvingCard,
    meta: alertMetaOf(item, title, status, resolvingCard),
  };
}

/** Alerts with trigger + data, read from alertsUrl (zero literals). */
export async function fetchAlerts(alertsUrl: string): Promise<Fetched<TodayAlert[]>> {
  const r = await fetchJson<AlertsResponse | AlertItem[] | null>(alertsUrl, READ_TIMEOUT_MS);
  if (!r.ok) return r;
  if (!r.value) return { ok: false, message: 'alerts API returned no body' };
  const alerts: TodayAlert[] = [];
  for (const item of alertItems(r.value)) {
    const alert = toTodayAlert(item, alerts.length);
    if (alert) alerts.push(alert);
  }
  return { ok: true, value: alerts };
}
