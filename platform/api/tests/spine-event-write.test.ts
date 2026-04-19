import { handleSpineEvent } from '../src/spine-event-write';

function fakeDbFactory() {
  const runs: any[] = [];
  const ctor = jest.fn((_p: string) => ({
    pragma: jest.fn(),
    prepare: (sql: string) => ({ run: (...args: any[]) => { runs.push({ sql, args }); } }),
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

describe('handleSpineEvent', () => {
  it('400s when event is missing', () => {
    const append = jest.fn();
    const res = fakeRes();
    handleSpineEvent(
      { body: {} } as any,
      res,
      {
        appendFileSync: append, chorusLogPath: '/log',
        now: () => 't',
        traceDbPath: '/db', DatabaseCtor: jest.fn() as any,
        ensureTraceTable: jest.fn(),
      },
    );
    expect(res.status_).toBe(400);
    expect(append).not.toHaveBeenCalled();
  });

  it('defaults role to "system" and writes a spine line on happy path', () => {
    const append = jest.fn();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'card.pulled' } } as any,
      res,
      {
        appendFileSync: append, chorusLogPath: '/p/chorus.log',
        now: () => 'now',
        traceDbPath: '/db', DatabaseCtor: jest.fn() as any,
        ensureTraceTable: jest.fn(),
      },
    );
    const entry = JSON.parse(append.mock.calls[0][1].trim());
    expect(entry.event).toBe('card.pulled');
    expect(entry.role).toBe('system');
    expect(entry.component).toBe('spine-service');
    expect(res.body_).toEqual({ ok: true });
  });

  it('carries through role and other fields to the entry', () => {
    const append = jest.fn();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'nudge.delivered', role: 'kade', card: 2205, target: 'silas' } } as any,
      res,
      {
        appendFileSync: append, chorusLogPath: '/log',
        now: () => 't',
        traceDbPath: '/db', DatabaseCtor: jest.fn() as any,
        ensureTraceTable: jest.fn(),
      },
    );
    const entry = JSON.parse(append.mock.calls[0][1].trim());
    expect(entry.role).toBe('kade');
    expect(entry.card).toBe(2205);
    expect(entry.target).toBe('silas');
  });

  it('swallows append errors silently (best-effort spine-log)', () => {
    const append = jest.fn(() => { throw new Error('disk full'); });
    const res = fakeRes();
    expect(() => handleSpineEvent(
      { body: { event: 'x' } } as any,
      res,
      {
        appendFileSync: append, chorusLogPath: '/log',
        now: () => 't',
        traceDbPath: '/db', DatabaseCtor: jest.fn() as any,
        ensureTraceTable: jest.fn(),
      },
    )).not.toThrow();
    expect(res.body_).toEqual({ ok: true });
  });

  it('inserts a trace hop when fields.hop is a number', () => {
    const { ctor, runs } = fakeDbFactory();
    const ensureTrace = jest.fn();
    const append = jest.fn();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'integration.x', hop: 3, domain: 'chorus', trace_id: 'corr-1', callStack: 'stack-a' } } as any,
      res,
      {
        appendFileSync: append, chorusLogPath: '/log',
        now: () => 'ts',
        traceDbPath: '/db', DatabaseCtor: ctor as any,
        ensureTraceTable: ensureTrace,
      },
    );
    expect(ensureTrace).toHaveBeenCalledTimes(1);
    expect(runs).toHaveLength(1);
    const a = runs[0].args;
    expect(a[0]).toBe('corr-1');
    expect(a[1]).toBe(3);
    expect(a[2]).toBe('stack-a');
    expect(a[3]).toBe('chorus');
  });

  it('does NOT insert a trace hop when hop field is absent', () => {
    const { ctor, runs } = fakeDbFactory();
    const ensureTrace = jest.fn();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'x' } } as any,
      res,
      {
        appendFileSync: jest.fn(), chorusLogPath: '/log',
        now: () => 'ts',
        traceDbPath: '/db', DatabaseCtor: ctor as any,
        ensureTraceTable: ensureTrace,
      },
    );
    expect(ensureTrace).not.toHaveBeenCalled();
    expect(runs).toHaveLength(0);
  });

  it('synthesizes a trace_id from Date.now() when not provided', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'x', hop: 1 } } as any,
      res,
      {
        appendFileSync: jest.fn(), chorusLogPath: '/log',
        now: () => 'ts',
        traceDbPath: '/db', DatabaseCtor: ctor as any,
        ensureTraceTable: jest.fn(),
      },
    );
    expect(runs[0].args[0]).toMatch(/^spine-\d+/);
  });

  it('does NOT insert when hop is NaN or not a number', () => {
    const { ctor, runs } = fakeDbFactory();
    const res = fakeRes();
    handleSpineEvent(
      { body: { event: 'x', hop: 'three' } } as any,
      res,
      {
        appendFileSync: jest.fn(), chorusLogPath: '/log',
        now: () => 'ts',
        traceDbPath: '/db', DatabaseCtor: ctor as any,
        ensureTraceTable: jest.fn(),
      },
    );
    expect(runs).toHaveLength(0);
  });
});
