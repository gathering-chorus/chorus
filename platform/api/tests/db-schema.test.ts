import { createRcaTableEnsurer, createTraceTableEnsurer } from '../src/db-schema';

function fakeDb() {
  return {
    pragma: jest.fn(),
    exec: jest.fn(),
    prepare: jest.fn(() => ({ get: jest.fn(() => undefined) })),
    close: jest.fn(),
  };
}

describe('createRcaTableEnsurer', () => {
  it('opens writable db, sets WAL pragma, and creates the rcas table', () => {
    const db = fakeDb();
    const DatabaseCtor = jest.fn(() => db);
    const ensure = createRcaTableEnsurer({ dbPath: '/rca.db', DatabaseCtor: DatabaseCtor as any });
    ensure();
    expect(DatabaseCtor).toHaveBeenCalledWith('/rca.db');
    expect(db.pragma).toHaveBeenCalledWith('journal_mode = WAL');
    const execSql = (db.exec.mock.calls[0]?.[0] as string) || '';
    expect(execSql).toContain('CREATE TABLE IF NOT EXISTS rcas');
    expect(execSql).toContain('title TEXT NOT NULL');
    expect(execSql).toContain('root_cause TEXT NOT NULL');
    expect(execSql).toContain("status TEXT DEFAULT 'open'");
  });

  it('closes the database after creating the table', () => {
    const db = fakeDb();
    const ensure = createRcaTableEnsurer({
      dbPath: '/x', DatabaseCtor: jest.fn(() => db) as any,
    });
    ensure();
    expect(db.close).toHaveBeenCalledTimes(1);
  });
});

describe('createTraceTableEnsurer', () => {
  it('creates traces table with expected columns', () => {
    const db = fakeDb();
    const DatabaseCtor = jest.fn(() => db);
    const ensure = createTraceTableEnsurer({ dbPath: '/trace.db', DatabaseCtor: DatabaseCtor as any });
    ensure();
    expect(db.pragma).toHaveBeenCalledWith('journal_mode = WAL');
    const execCalls = db.exec.mock.calls.map(c => c[0] as string);
    const createSql = execCalls.find(s => s.includes('CREATE TABLE IF NOT EXISTS traces')) || '';
    expect(createSql).toContain('correlation_id TEXT NOT NULL');
    expect(createSql).toContain('hop INTEGER NOT NULL');
    expect(createSql).toContain('call_stack TEXT NOT NULL');
  });

  it('creates both indexes when they are absent', () => {
    const db = fakeDb();
    // prepare().get() returns undefined → "index not found" → creates both.
    const ensure = createTraceTableEnsurer({
      dbPath: '/t', DatabaseCtor: jest.fn(() => db) as any,
    });
    ensure();
    const execSqls = db.exec.mock.calls.map(c => c[0] as string).join('\n');
    expect(execSqls).toContain('CREATE INDEX idx_traces_corr ON traces(correlation_id)');
    expect(execSqls).toContain('CREATE INDEX idx_traces_domain ON traces(source_domain)');
  });

  it('skips index creation when the idx_traces_corr index already exists', () => {
    const db = {
      pragma: jest.fn(),
      exec: jest.fn(),
      prepare: jest.fn(() => ({ get: jest.fn(() => ({ name: 'idx_traces_corr' })) })),
      close: jest.fn(),
    };
    const ensure = createTraceTableEnsurer({
      dbPath: '/t', DatabaseCtor: jest.fn(() => db) as any,
    });
    ensure();
    const createIndexCalls = db.exec.mock.calls
      .map(c => c[0] as string)
      .filter(s => s.includes('CREATE INDEX'));
    expect(createIndexCalls).toHaveLength(0);
  });

  it('closes the database after ensure', () => {
    const db = fakeDb();
    const ensure = createTraceTableEnsurer({
      dbPath: '/t', DatabaseCtor: jest.fn(() => db) as any,
    });
    ensure();
    expect(db.close).toHaveBeenCalledTimes(1);
  });
});
