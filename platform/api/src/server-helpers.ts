// Small server helpers (extracted from server.ts for #2205 wave 12):
// - DbNotFoundError + createDbOpener: readonly index-db factory.
// - createAlertFilesReader: .yml enumeration + read for domain alerts.
// - createSearchEventEmitter: fire-and-forget spine-event emitter.
// - crashAlert: stderr log marker (Silas checks at session start).

export class DbNotFoundError extends Error {
  constructor() { super('Chorus index database not found'); }
}

export interface DbOpenerDeps {
  dbPath: string;
  exists: (p: string) => boolean;
  DatabaseCtor: new (path: string, opts: any) => { pragma: (s: string) => void };
}

export function createDbOpener<T = any>(deps: DbOpenerDeps): () => T {
  return (): T => {
    if (!deps.exists(deps.dbPath)) {
      throw new DbNotFoundError();
    }
    const db = new deps.DatabaseCtor(deps.dbPath, { readonly: true });
    db.pragma('journal_mode = WAL');
    return db as unknown as T;
  };
}

export interface AlertFilesReaderDeps {
  fs: {
    readdirSync: (p: string) => string[];
    readFileSync: (p: string, enc: string) => any;
  };
  alertsDir: string;
}

export function createAlertFilesReader(
  deps: AlertFilesReaderDeps,
): () => Array<{ file: string; content: string }> {
  return () => deps.fs.readdirSync(deps.alertsDir)
    .filter(f => f.endsWith('.yml'))
    .map(f => ({
      file: f,
      content: String(deps.fs.readFileSync(`${deps.alertsDir}/${f}`, 'utf-8')),
    }));
}

export interface SearchEventEmitterDeps {
  chorusLogPath: string;
  execFileFn: (cmd: string, args: string[], opts: any, cb?: (...a: any[]) => void) => any;
}

export function createSearchEventEmitter(
  deps: SearchEventEmitterDeps,
): (fields: Record<string, string | number>) => void {
  return (fields: Record<string, string | number>) => {
    const args = [
      'search.query.executed',
      'system',
      ...Object.entries(fields).map(([k, v]) => `${k}=${v}`),
    ];
    deps.execFileFn(deps.chorusLogPath, args, { timeout: 5000 }, () => {});
  };
}

/**
 * Stderr marker for process crashes. Silas reviews at session start;
 * no nudges, no notifications.
 */
export function crashAlert(reason: string, errSink: (msg: string) => void = console.error): void {
  errSink(`[chorus-api] CRASH LOGGED: ${reason}`);
}
