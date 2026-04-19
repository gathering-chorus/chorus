import { handleRcaCreate, handleTraceCreate } from '../src/diagnostic-writes';

function fakeDbFactory() {
  const runs: any[] = [];
  const ctor = jest.fn((_p: string) => ({
    pragma: jest.fn(),
    prepare: (sql: string) => ({
      run: (...args: any[]) => {
        runs.push({ sql, args });
        return { lastInsertRowid: runs.length };
      },
    }),
    close: jest.fn(),
  }));
  return { ctor, runs };
}

function fakeRes() {
  const self: any = { status_: 200, body_: null };
  self.status = (s: number) => { self.status_ = s; return self; };
  self.json = (b: any) => { self.body_ = b; return self; };
  return self;
}

describe('handleRcaCreate', () => {
  it('400s when title is missing', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      { body: { trigger: 't', root_cause: 'r' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(res.status_).toBe(400);
    expect(res.body_.error).toMatch(/title, trigger, and root_cause/);
  });

  it('400s when trigger is missing', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', root_cause: 'r' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(res.status_).toBe(400);
  });

  it('ensures the table on first call via lazy-init deps.ensureTable', () => {
    const { ctor } = fakeDbFactory();
    const ensureTable = jest.fn();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', trigger: 'tg', root_cause: 'rc' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable,
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(ensureTable).toHaveBeenCalledTimes(1);
  });

  it('inserts with default status=open when status invalid', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', trigger: 'tg', root_cause: 'rc', status: 'banana' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    // status column is arg index 8.
    expect(runs[0].args[8]).toBe('open');
    expect(res.body_.status).toBe('open');
  });

  it('accepts valid custom status (verified, closed)', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', trigger: 'tg', root_cause: 'rc', status: 'verified' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(runs[0].args[8]).toBe('verified');
  });

  it('JSON-stringifies contributing/corrective/cards/spine_events', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      {
        body: {
          title: 't', trigger: 'tg', root_cause: 'rc',
          contributing_factors: ['a', 'b'],
          corrective_actions: ['c'],
          cards: [1, 2],
          spine_events: ['x'],
        },
      } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(runs[0].args[4]).toBe('["a","b"]');
    expect(runs[0].args[5]).toBe('["c"]');
    expect(runs[0].args[6]).toBe('[1,2]');
    expect(runs[0].args[7]).toBe('["x"]');
  });

  it('writes an rca.created spine event with the inserted id', () => {
    const { ctor } = fakeDbFactory();
    const append = jest.fn();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', trigger: 'tg', root_cause: 'rc', cards: [99] } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: append, chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(append).toHaveBeenCalledTimes(1);
    const entry = JSON.parse(append.mock.calls[0][1].trim());
    expect(entry.event).toBe('rca.created');
    expect(entry.component).toBe('rca');
    expect(entry.rca_id).toBe('1');
  });

  it('responds with ok + id + status on success', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleRcaCreate(
      { body: { title: 't', trigger: 'tg', root_cause: 'rc' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(),
        appendFileSync: jest.fn(), chorusLogPath: '/log', now: () => 'ts' },
    );
    expect(res.body_.ok).toBe(true);
    expect(res.body_.id).toBe(1);
    expect(res.body_.status).toBe('open');
  });
});

describe('handleTraceCreate', () => {
  it('400s when correlationId is missing', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      { body: { hop: 1, callStack: 's' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(res.status_).toBe(400);
  });

  it('400s when hop is missing', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      { body: { correlationId: 'c', callStack: 's' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(res.status_).toBe(400);
  });

  it('inserts with provided source/destination fields', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      {
        body: {
          correlationId: 'corr-1', hop: 1, callStack: 's',
          source: { domain: 'sd', service: 'ss', instance: 'si' },
          destination: { domain: 'dd', service: 'ds', instance: 'di' },
        },
      } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(runs[0].args[0]).toBe('corr-1');
    expect(runs[0].args[3]).toBe('sd');
    expect(runs[0].args[6]).toBe('dd');
  });

  it('nullifies source/destination fields when absent', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      { body: { correlationId: 'c', hop: 1, callStack: 's' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(runs[0].args[3]).toBeNull();
    expect(runs[0].args[6]).toBeNull();
  });

  it('includes error.classification + error.message when present', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      {
        body: { correlationId: 'c', hop: 1, callStack: 's',
          error: { classification: 'timeout', message: 'oops' } },
      } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(runs[0].args[11]).toBe('timeout');
    expect(runs[0].args[12]).toBe('oops');
  });

  it('responds ok true on successful insert', () => {
    const { ctor } = fakeDbFactory();
    const res = fakeRes();
    handleTraceCreate(
      { body: { correlationId: 'c', hop: 1, callStack: 's' } } as any,
      res,
      { dbPath: '/db', DatabaseCtor: ctor as any, ensureTable: jest.fn(), now: () => 'ts' },
    );
    expect(res.body_).toEqual({ ok: true });
  });
});
