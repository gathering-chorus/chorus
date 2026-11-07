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
  annotations?: Record<string, string>;
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

/** Labels that name the rule or its routing, not the series that fired. */
const RULE_LABELS = new Set(['alertname', 'role', 'grafana_folder']);

/**
 * The nudge text: the rule, the threshold and the values, nothing else.
 * #4717 (Jeff 10-24 06:03, "no more a=2 c=1") — Grafana's values are keyed by
 * query refId (A the query, B its reduce, C the threshold's 1/0), so `A=2 C=1`
 * named nothing a reader can use. The rule's `summary` annotation, when it has
 * one, says it in words with its values; without one, the series labels and
 * the query's value, never a refId.
 */
export function alertText(a: GrafanaAlert): string {
  const name = a.labels?.alertname || 'unnamed alert';
  const summary = (a.annotations?.summary || '').trim();
  if (summary) return `grafana: ${name} — ${summary}`;
  const series = Object.entries(a.labels || {})
    .filter(([k]) => !RULE_LABELS.has(k) && !k.startsWith('__'))
    .map(([k, v]) => `${k} ${v}`);
  // the first value is the query's (refIds come A, B, C); a bare valueString
  // carries it as `value=<n>` after `var='A'`
  const first = a.values && Object.keys(a.values).length > 0
    ? Object.values(a.values)[0]
    : (a.valueString || '').match(/value=(-?[0-9.eE+]+)/)?.[1];
  const parts = [...series, ...(first !== undefined && first !== '' ? [`value ${first}`] : [])];
  return parts.length ? `grafana: ${name} — ${parts.join(', ')}` : `grafana: ${name}`;
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
