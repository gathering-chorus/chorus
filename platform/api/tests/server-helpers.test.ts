import {
  createDbOpener,
  DbNotFoundError,
  createAlertFilesReader,
  createSearchEventEmitter,
  crashAlert,
} from '../src/server-helpers';

describe('DbNotFoundError', () => {
  it('is an Error subclass with a stable message', () => {
    const e = new DbNotFoundError();
    expect(e).toBeInstanceOf(Error);
    expect(e.message).toBe('Chorus index database not found');
  });
});

describe('createDbOpener', () => {
  it('throws DbNotFoundError when the db file does not exist', () => {
    const open = createDbOpener({
      dbPath: '/missing.db',
      exists: jest.fn(() => false),
      DatabaseCtor: jest.fn(),
    });
    expect(() => open()).toThrow(DbNotFoundError);
  });

  it('opens the db read-only and sets WAL pragma when the file exists', () => {
    const pragma = jest.fn();
    const fakeDb = { pragma };
    const DatabaseCtor = jest.fn(() => fakeDb);
    const open = createDbOpener({
      dbPath: '/present.db',
      exists: jest.fn(() => true),
      DatabaseCtor: DatabaseCtor as any,
    });
    const db = open();
    expect(DatabaseCtor).toHaveBeenCalledWith('/present.db', { readonly: true });
    expect(pragma).toHaveBeenCalledWith('journal_mode = WAL');
    expect(db).toBe(fakeDb);
  });
});

describe('createAlertFilesReader', () => {
  it('returns only .yml files from the alerts dir', () => {
    const fakeFs = {
      readdirSync: jest.fn(() => ['a.yml', 'b.txt', 'c.yml', 'README.md']),
      readFileSync: jest.fn((p: string) => `body of ${p}`),
    };
    const read = createAlertFilesReader({ fs: fakeFs as any, alertsDir: '/alerts' });
    const out = read();
    expect(out).toHaveLength(2);
    expect(out.map(o => o.file)).toEqual(['a.yml', 'c.yml']);
  });

  it('reads each file with utf-8 encoding', () => {
    const fakeFs = {
      readdirSync: jest.fn(() => ['one.yml']),
      readFileSync: jest.fn((_p: string, _enc: string) => 'contents'),
    };
    const read = createAlertFilesReader({ fs: fakeFs as any, alertsDir: '/al' });
    const out = read();
    expect(out[0].content).toBe('contents');
    expect(fakeFs.readFileSync).toHaveBeenCalledWith(expect.stringContaining('one.yml'), 'utf-8');
  });

  it('joins the alerts dir with each filename', () => {
    const fakeFs = {
      readdirSync: jest.fn(() => ['x.yml']),
      readFileSync: jest.fn((p: string) => p),
    };
    const read = createAlertFilesReader({ fs: fakeFs as any, alertsDir: '/custom/alerts' });
    const out = read();
    expect(out[0].content).toContain('/custom/alerts/x.yml');
  });
});

describe('createSearchEventEmitter', () => {
  it('invokes execFile with the chorus-log path + event + role + fields', () => {
    const execFileFn = jest.fn();
    const emit = createSearchEventEmitter({
      chorusLogPath: '/chorus-log',
      execFileFn: execFileFn as any,
    });
    emit({ mode: 'fts', limit: 5 });
    expect(execFileFn).toHaveBeenCalledTimes(1);
    const [cmd, args] = execFileFn.mock.calls[0];
    expect(cmd).toBe('/chorus-log');
    expect(args[0]).toBe('search.query.executed');
    expect(args[1]).toBe('system');
    expect(args).toContain('mode=fts');
    expect(args).toContain('limit=5');
  });

  it('passes a 5s timeout to execFile', () => {
    const execFileFn = jest.fn();
    const emit = createSearchEventEmitter({
      chorusLogPath: '/x', execFileFn: execFileFn as any,
    });
    emit({});
    const [, , opts] = execFileFn.mock.calls[0];
    expect(opts.timeout).toBe(5000);
  });

  it('emits with just the base args when fields is empty', () => {
    const execFileFn = jest.fn();
    const emit = createSearchEventEmitter({
      chorusLogPath: '/x', execFileFn: execFileFn as any,
    });
    emit({});
    const [, args] = execFileFn.mock.calls[0];
    expect(args).toEqual(['search.query.executed', 'system']);
  });
});

describe('crashAlert', () => {
  it('writes a CRASH LOGGED marker to the injected error sink', () => {
    const errSink = jest.fn();
    crashAlert('node exited with signal SIGTERM', errSink);
    expect(errSink).toHaveBeenCalledTimes(1);
    expect(errSink.mock.calls[0][0]).toContain('CRASH LOGGED');
    expect(errSink.mock.calls[0][0]).toContain('node exited with signal SIGTERM');
  });

  it('does nothing destructive — no throws, no other side effects', () => {
    const errSink = jest.fn();
    expect(() => crashAlert('anything', errSink)).not.toThrow();
  });
});
