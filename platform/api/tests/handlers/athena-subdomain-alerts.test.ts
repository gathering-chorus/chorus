/**
 * athena-subdomain-alerts handler — unit tests (#2187).
 *
 * Scans a filesystem directory of YAML alert files, filters by the domain
 * keyword (match in filename or content), parses basic fields via regex.
 */
import {
  fetchAthenaSubdomainAlerts,
  type AthenaSubdomainAlertsDeps,
} from '../../src/handlers/athena-subdomain-alerts';

function deps(overrides: Partial<AthenaSubdomainAlertsDeps> = {}): AthenaSubdomainAlertsDeps {
  return {
    listAlertFiles: () => [],
    readAlertFile: () => '',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubdomainAlerts (#2187)', () => {
  test('no alert files returns 200 with empty alerts list', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps(), 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { alerts: Array<unknown> } };
    expect(body.data.alerts).toEqual([]);
  });

  test('strips -domain/-service/-analytics suffix when building search keyword', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['chorus-heartbeat.yml'],
      readAlertFile: () => 'name: heartbeat\ndescription: chorus heartbeat',
    }), 'chorus-domain');
    const body = r.body as { data: { domainLabel: string; alerts: Array<unknown> } };
    expect(body.data.domainLabel).toBe('chorus');
    expect(body.data.alerts).toHaveLength(1);
  });

  test('filename match includes the alert', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['pulse-latency.yml'],
      readAlertFile: () => 'name: pulse-latency\ndescription: unrelated',
    }), 'pulse');
    const body = r.body as { data: { alerts: Array<unknown> } };
    expect(body.data.alerts).toHaveLength(1);
  });

  test('content-only match (keyword in description) includes the alert', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['other.yml'],
      readAlertFile: () => 'name: alarm\ndescription: PULSE went down',
    }), 'pulse');
    const body = r.body as { data: { alerts: Array<unknown> } };
    expect(body.data.alerts).toHaveLength(1);
  });

  test('no match excludes the alert', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['unrelated.yml'],
      readAlertFile: () => 'name: other\ndescription: nothing',
    }), 'pulse');
    const body = r.body as { data: { alerts: Array<unknown> } };
    expect(body.data.alerts).toEqual([]);
  });

  test('parses name, description, severity, schedule from YAML-ish content', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['pulse-lag.yml'],
      readAlertFile: () => 'name: pulse-lag\ndescription: Pulse taking >5s\nseverity: warning\nschedule: "*/5 * * * *"',
    }), 'pulse');
    const body = r.body as { data: { alerts: Array<{ name: string; description: string; severity: string; schedule: string; file: string }> } };
    expect(body.data.alerts[0]).toEqual({
      file: 'pulse-lag.yml',
      name: 'pulse-lag',
      description: 'Pulse taking >5s',
      severity: 'warning',
      schedule: '*/5 * * * *',
    });
  });

  test('missing fields get stable defaults (name from filename, description empty, severity unknown, schedule empty)', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => ['pulse-minimal.yml'],
      readAlertFile: () => 'pulse',
    }), 'pulse');
    const body = r.body as { data: { alerts: Array<{ name: string; description: string; severity: string; schedule: string }> } };
    expect(body.data.alerts[0].name).toBe('pulse-minimal');
    expect(body.data.alerts[0].description).toBe('');
    expect(body.data.alerts[0].severity).toBe('unknown');
    expect(body.data.alerts[0].schedule).toBe('');
  });

  test('listAlertFiles throws → 500 with error envelope', async () => {
    const r = await fetchAthenaSubdomainAlerts(deps({
      listAlertFiles: () => { throw new Error('ENOENT'); },
    }), 'x');
    expect(r.status).toBe(500);
    const body = r.body as { _meta: { error: boolean } };
    expect(body._meta.error).toBe(true);
  });
});
