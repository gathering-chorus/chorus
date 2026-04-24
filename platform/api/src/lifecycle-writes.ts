// Lifecycle write handlers (extracted from server.ts for #2205 wave 19).
// Three POST routes that share a write-to-spine-log shape:
// - pulse: event + role + optional extras → spine-log line
// - role-state: role + state → /tmp/role-state-<role>.json + spine-log line
// - alert: Grafana webhook → spine-log line per alert + optional desktop notify

import type { Request as Req, Response as Res } from 'express';

export interface PulseDeps {
  appendFileSync: typeof import('fs').appendFileSync;
  chorusLogPath: string;
  now: () => string;
}

export function handlePulse(req: Req, res: Res, deps: PulseDeps): void {
  const { event, role, level, ...extras } = req.body || {};
  if (!event || !role) {
    res.status(400).json!({ error: 'event and role are required' });
    return;
  }
  const validLevels = ['info', 'warn', 'critical'];
  const safeLevel = validLevels.includes(level) ? level : 'info';
  const entry: Record<string, string> = {
    timestamp: deps.now(),
    level: safeLevel,
    appName: 'chorus-events',
    component: 'lifecycle',
    event,
    role,
  };
  for (const [k, v] of Object.entries(extras)) {
    if (typeof v === 'string' || typeof v === 'number') {
      entry[k] = String(v);
    }
  }
  deps.appendFileSync(deps.chorusLogPath, JSON.stringify(entry) + '\n');
  res.json!({ ok: true, event, role, level: safeLevel });
}

export interface RoleStateDeps {
  appendFileSync: typeof import('fs').appendFileSync;
  writeFileSync: typeof import('fs').writeFileSync;
  chorusLogPath: string;
}

export function handleRoleState(req: Req, res: Res, deps: RoleStateDeps): void {
  const { role, state, card, type: cardType } = req.body || {};
  if (!role || !state) {
    res.status(400).json!({ error: 'role and state are required' });
    return;
  }
  const validStates = ['building', 'blocked', 'waiting', 'observing', 'idle'];
  if (!validStates.includes(state)) {
    res.status(400).json!({ error: `Invalid state '${state}'. Use: ${validStates.join(', ')}` });
    return;
  }
  const stateFile = `/tmp/role-state-${role}.json`;
  const ts = new Date().toISOString();
  const stateData = { role, state, card: card ?? null, type: cardType || null, updated: ts };
  deps.writeFileSync(stateFile, JSON.stringify(stateData, null, 2));

  const entry = JSON.stringify({
    timestamp: ts,
    level: 'info',
    appName: 'chorus-events',
    component: 'lifecycle',
    event: 'role.state.changed',
    role,
    state,
    ...(card !== undefined && card !== null ? { card: String(card) } : {}),
    ...(cardType ? { type: cardType } : {}),
  });
  deps.appendFileSync(deps.chorusLogPath, entry + '\n');
  res.json!({ ok: true, role, state, card: card ?? null });
}

export interface AlertDeps {
  appendFileSync: typeof import('fs').appendFileSync;
  notify: (title: string, message: string) => void;
  chorusLogPath: string;
}

export function handleAlert(req: Req, res: Res, deps: AlertDeps): void {
  const alerts = req.body?.alerts || [];
  const ts = new Date().toISOString();
  for (const alert of alerts) {
    const severity = alert.labels?.severity || 'unknown';
    const alertname = alert.labels?.alertname || 'unknown';
    const status = alert.status || 'unknown';
    const summary = alert.annotations?.summary || '';
    const description = alert.annotations?.description || '';
    const entry = JSON.stringify({
      timestamp: ts,
      level: severity === 'critical' ? 'error' : 'warn',
      appName: 'grafana-alerts',
      component: 'alertmanager',
      event: `alert_${status}`,
      role: 'system',
      alertname,
      severity,
      summary,
      description: description.substring(0, 500),
    });
    deps.appendFileSync(deps.chorusLogPath, entry + '\n');
    if (severity === 'critical' && status === 'firing') {
      deps.notify(`ALERT: ${alertname}`, summary || description.substring(0, 100));
    }
  }
  res.json!({ received: alerts.length });
}
