/**
 * athena-machines handler — unit tests (#2187).
 *
 * Machines with services running on them. SPARQL returns one row per
 * (machine, service) pair; handler dedupes by machine and accumulates services.
 */
import {
  fetchAthenaMachines,
  type AthenaMachinesDeps,
  type SparqlMachineBinding,
} from '../../src/handlers/athena-machines';

function result(bindings: SparqlMachineBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaMachinesDeps> = {}): AthenaMachinesDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaMachines (#2187)', () => {
  test('empty result returns 200 with empty array', async () => {
    const r = await fetchAthenaMachines(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('machine with two services dedupes into one record with two services', async () => {
    const r = await fetchAthenaMachines(deps({
      sparql: async () => result([
        {
          machine: { value: 'https://jeffbridwell.com/chorus#library' },
          label: { value: 'Library' },
          ip: { value: '192.168.86.36' },
          role: { value: 'primary' },
          service: { value: 'https://jeffbridwell.com/chorus#fuseki' },
          serviceLabel: { value: 'Fuseki' },
        },
        {
          machine: { value: 'https://jeffbridwell.com/chorus#library' },
          label: { value: 'Library' },
          ip: { value: '192.168.86.36' },
          role: { value: 'primary' },
          service: { value: 'https://jeffbridwell.com/chorus#gathering-app' },
          serviceLabel: { value: 'Gathering App' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string; services: Array<{ label: string }> }> };
    expect(body.data).toHaveLength(1);
    expect(body.data[0].label).toBe('Library');
    expect(body.data[0].services.map((s) => s.label)).toEqual(['Fuseki', 'Gathering App']);
  });

  test('machine without services still produces a record with empty services', async () => {
    const r = await fetchAthenaMachines(deps({
      sparql: async () => result([
        {
          machine: { value: 'https://jeffbridwell.com/chorus#empty-host' },
          label: { value: 'Empty Host' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string; services: Array<unknown>; ip: string | null; role: string | null }> };
    expect(body.data).toHaveLength(1);
    expect(body.data[0].services).toEqual([]);
    expect(body.data[0].ip).toBeNull();
    expect(body.data[0].role).toBeNull();
  });

  test('missing labels fall back to URI fragment', async () => {
    const r = await fetchAthenaMachines(deps({
      sparql: async () => result([
        {
          machine: { value: 'https://jeffbridwell.com/chorus#bare-machine' },
          service: { value: 'https://jeffbridwell.com/chorus#bare-service' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string; services: Array<{ label: string }> }> };
    expect(body.data[0].label).toBe('bare-machine');
    expect(body.data[0].services[0].label).toBe('bare-service');
  });

  test('two machines produce two records', async () => {
    const r = await fetchAthenaMachines(deps({
      sparql: async () => result([
        { machine: { value: '#a' }, label: { value: 'A' } },
        { machine: { value: '#b' }, label: { value: 'B' } },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string }>; _meta: { count: number } };
    expect(body.data.map((m) => m.label)).toEqual(['A', 'B']);
    expect(body._meta.count).toBe(2);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaMachines(deps({
      sparql: async () => { throw new Error('fuseki unreachable'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('fuseki unreachable');
    expect(body._meta.error).toBe(true);
  });

  test('loadQuery is called with name "machines"', async () => {
    let seenName = '';
    await fetchAthenaMachines(deps({
      loadQuery: (name) => { seenName = name; return '# q'; },
    }));
    expect(seenName).toBe('machines');
  });
});
