/**
 * context-alerts handler tests (#2252).
 */

import {
  fetchContextAlerts,
  type ContextAlertsDeps,
} from '../../src/handlers/context-alerts';

function stubSparql(): ContextAlertsDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

function deps(
  pulseJson: string | null,
  alertFiles: Record<string, string> = {},
): ContextAlertsDeps {
  return {
    sparql: stubSparql(),
    readPulse: () => pulseJson,
    listAlertFiles: () => Object.keys(alertFiles),
    readAlertFile: (name) => alertFiles[name] ?? null,
  };
}

const YAML_FUSEKI = `name: fuseki-harvest-stale
description: No new harvest data in Fuseki — pipeline may be broken
severity: warning
`;

const YAML_VIKUNJA = `name: vikunja-auth-failure
description: Vikunja auth rejected
severity: critical
`;

describe('fetchContextAlerts', () => {
  it('returns fired alerts from pulse with metadata from yaml', async () => {
    const pulse = JSON.stringify({
      alerts: { count: 2, fired_today: ['fuseki-harvest-stale', 'vikunja-auth-failure'] },
    });
    const r = await fetchContextAlerts(
      deps(pulse, {
        'fuseki-harvest-stale.yml': YAML_FUSEKI,
        'vikunja-auth-failure.yml': YAML_VIKUNJA,
      }),
      '/api/chorus/context/alerts',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; alerts: Array<{ name: string; severity: string; description: string }> } };
    expect(body.data.total).toBe(2);
    expect(body.data.alerts).toHaveLength(2);
    const fuseki = body.data.alerts.find((a) => a.name === 'fuseki-harvest-stale')!;
    expect(fuseki.severity).toBe('warning');
    expect(fuseki.description).toMatch(/No new harvest data/);
    const vik = body.data.alerts.find((a) => a.name === 'vikunja-auth-failure')!;
    expect(vik.severity).toBe('critical');
  });

  it('fired alert without yaml still returns with unknown severity', async () => {
    const pulse = JSON.stringify({
      alerts: { fired_today: ['orphan-alert'] },
    });
    const r = await fetchContextAlerts(deps(pulse), '/api/chorus/context/alerts');
    const body = r.body as { data: { alerts: Array<{ name: string; severity: string }> } };
    expect(body.data.alerts[0].name).toBe('orphan-alert');
    expect(body.data.alerts[0].severity).toBe('unknown');
  });

  it('empty fired list → empty alerts array, status 200', async () => {
    const pulse = JSON.stringify({ alerts: { count: 0, fired_today: [] } });
    const r = await fetchContextAlerts(deps(pulse), '/api/chorus/context/alerts');
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; alerts: unknown[] } };
    expect(body.data.total).toBe(0);
    expect(body.data.alerts).toEqual([]);
  });

  it('pulse missing alerts block → empty, not crash', async () => {
    const pulse = JSON.stringify({});
    const r = await fetchContextAlerts(deps(pulse), '/api/chorus/context/alerts');
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number } };
    expect(body.data.total).toBe(0);
  });

  it('503 when pulse missing', async () => {
    const r = await fetchContextAlerts(deps(null), '/api/chorus/context/alerts');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/pulse/i);
  });

  it('500 when pulse unparseable', async () => {
    const r = await fetchContextAlerts(deps('not json'), '/api/chorus/context/alerts');
    expect(r.status).toBe(500);
  });

  it('envelope carries source + timestamp + domain=chorus', async () => {
    const pulse = JSON.stringify({ alerts: { fired_today: [] } });
    const r = await fetchContextAlerts(deps(pulse), '/api/chorus/context/alerts');
    const body = r.body as { source: string; timestamp: string; domain?: string };
    expect(body.source).toBe('/api/chorus/context/alerts');
    expect(body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(body.domain).toBe('chorus');
  });
});
