// @test-type: unit — pure functions over door-shaped rows; no store, no network
// #4405 reopen 4 — Visible (Jeff, 2026-10-12: "make sure our runner is properly
// aligned integrated and visible as a sensor"). /crawler-validate shows each
// Sensor's last run: when, on what, what it changed. A sensor that has not
// looked within its window reads stale, never green.
import { collectionOf, sensorStates, renderSensorsSection, loadSensors } from '../src/handlers/sensors-panel';

const NOW = Date.parse('2026-10-13T02:00:00Z');
const sensor = (name: string, window: number, owner: string) => ({
  name, sensorPass: name, sensorWindowMinutes: String(window), ownedBy: `principal-${owner}`, sensorWrites: ['Machine', 'Disk'],
});
// the door's served shape: edges carry the kind prefix, numbers come as strings
const run = (name: string, sensorName: string, started: string, outcome: string, ended = '', counts = ['0', '0', '0', '32']) => ({
  name, runOfSensor: `sensor-${sensorName}`, runTarget: 'library', startedAt: started, sensorEndedAt: ended, sensorOutcome: outcome,
  plannedCreate: counts[0], plannedUpdate: counts[1], plannedDelete: counts[2], plannedUnchanged: counts[3], requestedBy: 'principal-silas',
});

describe('#4405 the sensors panel', () => {
  test('the door names the collections; nothing is guessed', () => {
    const prims = [{ kind: 'Sensor', collection: '/v1/heralds/sensors' }, { kind: 'SensorRun', collection: '/v1/heralds/sensorruns' }];
    expect(collectionOf(prims, 'Sensor')).toBe('/v1/heralds/sensors');
    expect(collectionOf(prims, 'Machine')).toBeNull();
  });

  test('each sensor shows its latest run, with what it changed', () => {
    const rows = sensorStates(
      [sensor('infra', 60, 'silas')],
      [run('r1', 'infra', '2026-10-13T00:10:00Z', 'green', '2026-10-13T00:10:02Z'),
        run('r2', 'infra', '2026-10-13T01:40:00Z', 'green', '2026-10-13T01:40:03Z', ['0', '2', '0', '30'])],
      NOW,
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ sensor: 'infra', owner: 'silas', state: 'green', run: 'r2', target: 'library', update: 2, unchanged: 30 });
  });

  test('NEGATIVE PROOF — overdue reads stale, a never-run sensor is not green, a red run is red', () => {
    const rows = sensorStates(
      [sensor('infra', 60, 'silas'), sensor('code', 1440, 'kade'), sensor('stores', 60, 'silas')],
      [run('a', 'infra', '2026-10-12T20:00:00Z', 'green', '2026-10-12T20:00:02Z'),
        run('b', 'stores', '2026-10-13T01:50:00Z', 'red', '2026-10-13T01:50:01Z')],
      NOW,
    );
    const by = Object.fromEntries(rows.map((r) => [r.sensor, r.state]));
    expect(by).toEqual({ infra: 'stale', code: 'never', stores: 'red' });
  });

  test('NEGATIVE PROOF — a run stuck at running past its window is stale, not running forever', () => {
    const rows = sensorStates([sensor('infra', 60, 'silas')], [run('k', 'infra', '2026-10-12T22:00:00Z', 'running')], NOW);
    expect(rows[0].state).toBe('stale');
    const live = sensorStates([sensor('infra', 60, 'silas')], [run('k', 'infra', '2026-10-13T01:55:00Z', 'running')], NOW);
    expect(live[0].state).toBe('running');
  });

  test('the section says so when the door does not serve sensors, and never shows green', () => {
    const html = renderSensorsSection({ measured: false, reason: 'the door does not serve Sensor' });
    expect(html).toContain('UNMEASURED');
    expect(html).not.toContain('pill green');
    const ok = renderSensorsSection({ measured: true, rows: sensorStates([sensor('infra', 60, 'silas')], [run('r2', 'infra', '2026-10-13T01:40:00Z', 'green', '2026-10-13T01:40:03Z')], NOW) });
    expect(ok).toContain('infra');
    expect(ok).toContain('pill green');
    expect(ok).toContain('borg-tend-herald');
  });

  test('reopen 5 — the owner comes from the row itself, in the door\'s served shape', async () => {
    // measured on prod 2026-10-13: the list omits ownedBy; GET /<coll>/<name> carries links.ownedBy
    const pages: Record<string, unknown> = {
      '/': { primitives: [{ kind: 'Sensor', collection: '/v1/heralds/sensors' }, { kind: 'SensorRun', collection: '/v1/heralds/sensorruns' }] },
      '/v1/heralds/sensors?limit=500': { data: [{ name: 'infra', sensorPass: 'infra', sensorWindowMinutes: '60' }], links: {} },
      '/v1/heralds/sensorruns?limit=500': { data: [run('infra-20261013t095711z', 'infra', '2026-10-13T01:57:11Z', 'green', '2026-10-13T01:57:13Z')], links: {} },
      '/v1/heralds/sensors/infra': { data: {}, links: { ownedBy: 'chorus:principal-silas' } },
    };
    const fake = (async (url: string) => {
      const body = pages[url.replace('http://door', '')];
      return { ok: body !== undefined, status: body ? 200 : 404, json: async () => body } as Response;
    }) as unknown as typeof fetch;
    const panel = await loadSensors('http://door', NOW, fake);
    expect(panel).toMatchObject({ measured: true });
    if (!panel.measured) return;
    expect(panel.rows[0]).toMatchObject({ sensor: 'infra', owner: 'silas', state: 'green' });
    // NEGATIVE PROOF: without the row's own read the owner is blank, which is what prod showed
    delete pages['/v1/heralds/sensors/infra'];
    expect(await loadSensors('http://door', NOW, fake)).toMatchObject({ measured: false });
  });
});
