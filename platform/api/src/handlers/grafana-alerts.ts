/**
 * #4666 — Grafana's alerts reach a role. Until 10-20 the root notification
 * policy sent every alert to the receiver `empty` (no contact points): no
 * Grafana rule reached anyone. The webhook contact point posts here; each
 * firing alert whose rule names a `role` label becomes one ops nudge to that
 * role, carrying the trigger and its values only (Jeff 10-16: never the why).
 *
 * An alert with no `role` label goes nowhere, visibly: it is listed as
 * unrouted and written to the spine, never broadcast (Wren 10-20 17:15 — when
 * the policy stopped sending to `empty`, every firing rule would page at once).
 */

export const ALERT_ROLES = ['silas', 'wren', 'kade', 'abby-normal', 'jeff'] as const;

export interface GrafanaAlert {
  status?: string;
  labels?: Record<string, string>;
  values?: Record<string, number | string> | null;
  valueString?: string;
}

export interface GrafanaPayload {
  alerts?: GrafanaAlert[];
}

export interface RouteDeps {
  nudge: (to: string, text: string) => void;
  spine: (event: string, fields: Record<string, string>) => void;
}

export interface RouteResult {
  routed: Array<{ role: string; text: string }>;
  unrouted: string[];
}

/** The nudge text: the rule's name and its values, nothing else. */
export function alertText(a: GrafanaAlert): string {
  const name = a.labels?.alertname || 'unnamed alert';
  const values = a.values && Object.keys(a.values).length > 0
    ? Object.entries(a.values).map(([k, v]) => `${k}=${v}`).join(' ')
    : (a.valueString || '').trim();
  return values ? `grafana: ${name} — ${values}` : `grafana: ${name}`;
}

export function routeGrafanaAlerts(payload: GrafanaPayload, deps: RouteDeps): RouteResult {
  const result: RouteResult = { routed: [], unrouted: [] };
  for (const a of payload.alerts || []) {
    if (a.status !== 'firing') continue; // a resolved alert pages no one
    const name = a.labels?.alertname || 'unnamed alert';
    const role = a.labels?.role || '';
    if (!(ALERT_ROLES as readonly string[]).includes(role)) {
      result.unrouted.push(name);
      deps.spine('alert.unrouted', { alertname: name, role: role || '(none)' });
      continue;
    }
    const text = alertText(a);
    deps.nudge(role, text);
    deps.spine('alert.routed', { alertname: name, role });
    result.routed.push({ role, text });
  }
  return result;
}
