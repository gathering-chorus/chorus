/* eslint-disable security/detect-object-injection --
 * Per-source indexer registry keyed by source enum.
 */
// indexAllSources (extracted from server.ts for #2205 wave 18).
//
// Biggest single extraction of the card. 11 independent source indexers
// share a prepared-statement + transaction context, each wrapped in
// try/catch so one source's failure doesn't tank the others. Factory
// form with injected fs / path / dbPath / homedir keeps tests hermetic.

/** Prepared-statement method — bindings passed positionally. any used because
 *  better-sqlite3 Statement<T> has a strict parameterized signature that doesn't
 *  structurally unify with a generic shim here. Callers cast results to their
 *  specific row type. #2463 scope: this is the remaining structural gap. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type StmtMethod = (...args: any[]) => any;

interface IndexStmt {
  run?: StmtMethod;
  all?: StmtMethod;
  get?: StmtMethod;
}

interface IndexDb {
  pragma: (s: string) => void;
  prepare: (sql: string) => IndexStmt;
  transaction: <T>(fn: (arg: T) => void) => (arg: T) => void;
  close: () => void;
}

export interface IndexAllSourcesDeps {
  dbPath: string;
  DatabaseCtor: new (path: string) => IndexDb;
  fs: {
    existsSync: (p: string) => boolean;
    readdirSync: (p: string) => string[];
    readFileSync: (p: string, enc: BufferEncoding) => string;
    statSync: (p: string) => { mtime: { toISOString: () => string } };
  };
  path: {
    join: (...parts: string[]) => string;
  };
  repoRoot: string;
  homedir: () => string;
  now?: () => string;
  // #3067: read only the recent tail of the (170MB) spine log, not the whole file.
  // Optional — falls back to a readFileSync slice for small logs / tests.
  readTail?: (path: string, maxBytes: number) => string;
  // #3077 AC2: read only the bytes appended since `offset` (incremental reindex).
  // Returns the decoded new content, the offset actually used (0 if the file was
  // rotated/truncated so `offset` > size), and the current byte size. Optional —
  // falls back to a readFileSync-from-offset for tests / pre-wire systems.
  readSince?: (path: string, offset: number) => { content: string; startOffset: number; size: number };
}

interface IndexCtx {
  db: IndexDb;
  insert: IndexStmt;
  updateWatermark: IndexStmt;
  fs: IndexAllSourcesDeps['fs'];
  path: IndexAllSourcesDeps['path'];
  repoRoot: string;
  homedir: () => string;
  now: string;
  readTail: (path: string, maxBytes: number) => string;
  readSince: (path: string, offset: number) => { content: string; startOffset: number; size: number };
}

// #3067 bounded indexSpine to the recent 16MB tail (down from the whole 170MB log).
// #3077 AC2 supersedes that: read only the bytes appended since the last cycle's
// persisted offset, so the JSON.parse cost is O(new) instead of O(16MB) per reindex.

/** Indexed spine event row — columns the insert statement binds. */
interface IndexEvent {
  source: string;
  source_id: string;
  channel: string;
  role: string;
  author: string;
  content: string;
  timestamp: string;
}

function runSource(name: string, results: Record<string, string>, fn: () => string | void): void {
  try {
    const summary = fn();
    if (summary) results[name] = summary;
  } catch (err: unknown) {
    results[name] = `error: ${err instanceof Error ? err.message : String(err)}`;
  }
}

function indexSpine(ctx: IndexCtx): string | void {
  const logPath = `${process.env.HOME}/.chorus/chorus.log`;
  if (!ctx.fs.existsSync(logPath)) return;
  // #3077 AC2: read ONLY the bytes appended since the last cycle, not the whole 16MB
  // tail every reindex. #3067 bounded the read to 16MB, but the JSON.parse of that
  // overlap re-ran every ~15-min cycle (the residual multi-second block, since the
  // insert is INSERT OR IGNORE so re-inserting was already a no-op — the PARSE was
  // the cost). The byte offset is persisted in watermarks('spine:offset').
  const offsetRow = ctx.db.prepare('SELECT last_indexed FROM watermarks WHERE source = ?')
    .get!('spine:offset') as { last_indexed?: string } | undefined;
  const storedOffset = offsetRow?.last_indexed ? (parseInt(offsetRow.last_indexed, 10) || 0) : 0;
  // readSince resets startOffset to 0 if the log was rotated/truncated (offset > size).
  const { content, startOffset } = ctx.readSince(logPath, storedOffset);
  // Process only COMPLETE (newline-terminated) lines; a trailing partial line — or a
  // byte range that split a multibyte char at the end — is left for the next cycle.
  // newOffset advances only past the last complete line, so nothing is dropped or
  // double-counted across cycles.
  const lastNl = content.lastIndexOf('\n');
  const complete = lastNl >= 0 ? content.slice(0, lastNl + 1) : '';
  const newOffset = startOffset + Buffer.byteLength(complete, 'utf-8');
  const lines = complete.length ? complete.trimEnd().split('\n') : [];
  let indexed = 0;
  const insertMany = ctx.db.transaction((events: IndexEvent[]) => {
    for (const e of events) {
      ctx.insert.run!(e.source, e.source_id, e.channel, e.role, e.author, e.content, e.timestamp);
      indexed++;
    }
  });
  // #2323: exclude self-referential search telemetry — the index was eating
  // its own tail (4.8% of spine volume; ~40% of a typical query result was
  // the query-log echoing itself). Extend if another event type emits
  // search-of-search.
  const EXCLUDED_EVENTS = new Set([
    'search.query.executed',
    'search.result.returned',
    'search.hierarchy.enrichment',
  ]);
  const events: IndexEvent[] = [];
  for (const line of lines) {
    try {
      const evt = JSON.parse(line);
      const role = evt.role || 'system';
      const event = evt.event || 'unknown';
      if (EXCLUDED_EVENTS.has(event)) continue;
      events.push({
        source: 'spine',
        source_id: `spine-${evt.timestamp}-${event}`,
        channel: `spine:${role}`,
        role, author: role, content: line,
        timestamp: evt.timestamp || ctx.now,
      });
    } catch { /* skip malformed */ }
  }
  insertMany(events);
  // Persist the new byte offset (last_indexed column is what the next cycle reads)
  // and the freshness timestamp on the 'spine' row.
  ctx.updateWatermark.run!('spine:offset', ctx.now, String(newOffset));
  ctx.updateWatermark.run!('spine', ctx.now, ctx.now);
  return `${indexed} events indexed`;
}

function indexRoleDir(
  ctx: IndexCtx,
  opts: { subpath: string; source: string; channelPrefix: string; fileFilter?: (f: string) => boolean; useMtime?: boolean },
): number {
  let indexed = 0;
  const filter = opts.fileFilter ?? ((f: string) => f.endsWith('.md') && !f.startsWith('.'));
  for (const role of ['wren', 'silas', 'kade']) {
    const dir = ctx.path.join(ctx.repoRoot, `roles/${role}/${opts.subpath}`);
    if (!ctx.fs.existsSync(dir)) continue;
    for (const file of ctx.fs.readdirSync(dir).filter(filter)) {
      const filePath = ctx.path.join(dir, file);
      const content = String(ctx.fs.readFileSync(filePath, 'utf-8'));
      const ts = opts.useMtime ? ctx.fs.statSync(filePath).mtime.toISOString() : ctx.now;
      ctx.insert.run!(opts.source, `${opts.source}:${role}:${file}`, `${opts.channelPrefix}:${role}`, role, role, content, ts);
      indexed++;
    }
  }
  return indexed;
}

function indexBriefs(ctx: IndexCtx): string {
  const indexed = indexRoleDir(ctx, { subpath: 'briefs', source: 'brief', channelPrefix: 'brief', useMtime: true });
  ctx.updateWatermark.run!('artifact:brief', ctx.now, ctx.now);
  return `${indexed} briefs indexed`;
}

function indexDecisions(ctx: IndexCtx): string | void {
  const decPath = ctx.path.join(ctx.repoRoot, 'roles/wren/decisions.md');
  if (!ctx.fs.existsSync(decPath)) return;
  const content = String(ctx.fs.readFileSync(decPath, 'utf-8'));
  const decisions = content.split('\n## DEC-').filter(Boolean);
  let indexed = 0;
  for (const dec of decisions) {
    const id = dec.split('\n')[0].match(/^(\d+)/)?.[1] || 'unknown';
    ctx.insert.run!('decision', `decision:DEC-${id}`, 'decisions', 'wren', 'wren', `## DEC-${dec}`, ctx.now);
    indexed++;
  }
  ctx.updateWatermark.run!('artifact:decisions', ctx.now, ctx.now);
  return `${indexed} decisions indexed`;
}

function indexAdrs(ctx: IndexCtx): string | void {
  const adrDir = ctx.path.join(ctx.repoRoot, 'roles/silas/adr');
  if (!ctx.fs.existsSync(adrDir)) return;
  const files = ctx.fs.readdirSync(adrDir).filter((f) => f.endsWith('.md'));
  let indexed = 0;
  for (const file of files) {
    const content = String(ctx.fs.readFileSync(ctx.path.join(adrDir, file), 'utf-8'));
    ctx.insert.run!('adr', `adr:${file}`, 'adr:silas', 'silas', 'silas', content, ctx.now);
    indexed++;
  }
  ctx.updateWatermark.run!('artifact:adr', ctx.now, ctx.now);
  return `${indexed} ADRs indexed`;
}

function indexActivity(ctx: IndexCtx): string | void {
  const actPath = ctx.path.join(ctx.repoRoot, 'activity.md');
  if (!ctx.fs.existsSync(actPath)) return;
  const content = String(ctx.fs.readFileSync(actPath, 'utf-8'));
  ctx.insert.run!('activity', 'activity:latest', 'activity', 'system', 'system', content, ctx.now);
  ctx.updateWatermark.run!('artifact:activity', ctx.now, ctx.now);
  return 'indexed';
}

function indexMemory(ctx: IndexCtx): string | void {
  const memDir = ctx.path.join(ctx.homedir(), '.claude/projects');
  if (!ctx.fs.existsSync(memDir)) return;
  let indexed = 0;
  for (const dir of ctx.fs.readdirSync(memDir).filter((d) => d.includes('chorus'))) {
    const memoryDir = ctx.path.join(memDir, dir, 'memory');
    if (!ctx.fs.existsSync(memoryDir)) continue;
    for (const file of ctx.fs.readdirSync(memoryDir).filter((f) => f.endsWith('.md'))) {
      const content = String(ctx.fs.readFileSync(ctx.path.join(memoryDir, file), 'utf-8'));
      ctx.insert.run!('memory', `memory:${file}`, 'memory', 'system', 'system', content, ctx.now);
      indexed++;
    }
  }
  ctx.updateWatermark.run!('artifact:memory', ctx.now, ctx.now);
  return `${indexed} memory files indexed`;
}

function indexState(ctx: IndexCtx): string {
  let indexed = 0;
  for (const role of ['wren', 'silas', 'kade']) {
    const nsPath = ctx.path.join(ctx.repoRoot, `roles/${role}/next-session.md`);
    if (!ctx.fs.existsSync(nsPath)) continue;
    const content = String(ctx.fs.readFileSync(nsPath, 'utf-8'));
    ctx.insert.run!('state', `state:${role}:next-session`, `state:${role}`, role, role, content, ctx.now);
    indexed++;
  }
  ctx.updateWatermark.run!('artifact:state', ctx.now, ctx.now);
  return `${indexed} state files indexed`;
}

function indexClearing(ctx: IndexCtx): string | void {
  const chatDir = '/tmp/chorus-chat';
  if (!ctx.fs.existsSync(chatDir)) return;
  let indexed = 0;
  for (const file of ctx.fs.readdirSync(chatDir).filter((f) => f.endsWith('.md'))) {
    const filePath = ctx.path.join(chatDir, file);
    const content = String(ctx.fs.readFileSync(filePath, 'utf-8'));
    const stat = ctx.fs.statSync(filePath);
    ctx.insert.run!('clearing', `clearing:${file}`, 'clearing:session', 'system', 'system', content, stat.mtime.toISOString());
    indexed++;
  }
  ctx.updateWatermark.run!('clearing', ctx.now, ctx.now);
  return `${indexed} transcripts indexed`;
}

function indexJournal(ctx: IndexCtx): string {
  const indexed = indexRoleDir(ctx, {
    subpath: 'journal',
    source: 'journal',
    channelPrefix: 'journal',
    fileFilter: (f) => f.endsWith('.md'),
  });
  ctx.updateWatermark.run!('journal', ctx.now, ctx.now);
  return `${indexed} journal entries indexed`;
}

function indexStories(ctx: IndexCtx): string {
  let indexed = 0;
  const storiesFile = ctx.path.join(ctx.repoRoot, 'roles/wren/self-stories.md');
  if (ctx.fs.existsSync(storiesFile)) {
    const content = String(ctx.fs.readFileSync(storiesFile, 'utf-8'));
    for (const story of content.split('\n## ').filter(Boolean)) {
      const title = story.split('\n')[0].trim();
      const id = title.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase().slice(0, 50);
      ctx.insert.run!('story', `story:${id}`, 'stories', 'wren', 'jeff', `## ${story}`, ctx.now);
      indexed++;
    }
  }
  const archiveDir = ctx.path.join(ctx.repoRoot, 'roles/wren/briefs-archive');
  if (ctx.fs.existsSync(archiveDir)) {
    for (const file of ctx.fs.readdirSync(archiveDir).filter((f) => f.includes('story'))) {
      const content = String(ctx.fs.readFileSync(ctx.path.join(archiveDir, file), 'utf-8'));
      ctx.insert.run!('story', `story:brief:${file}`, 'stories', 'wren', 'jeff', content, ctx.now);
      indexed++;
    }
  }
  ctx.updateWatermark.run!('stories', ctx.now, ctx.now);
  return `${indexed} stories indexed`;
}

function clearSlackWatermarks(db: IndexDb, results: Record<string, string>): void {
  try {
    db.prepare('DELETE FROM watermarks WHERE source LIKE \'slack%\'').run!();
    db.prepare('DELETE FROM watermarks WHERE source = \'slack\'').run!();
    results.slack = 'removed (deprecated)';
  } catch { /* ignore */ }
}

export function createIndexAllSources(deps: IndexAllSourcesDeps): () => Promise<{ indexed: Record<string, string>; elapsed_ms: number }> {
  const nowFn = deps.now ?? (() => new Date().toISOString());
  // eslint-disable-next-line @typescript-eslint/require-await -- signature preserved as Promise for async-swap later if any source goes async
  return async function indexAllSources() {
    const db = new deps.DatabaseCtor(deps.dbPath);
    db.pragma('journal_mode = WAL');
    // #3085: reindex now runs in a standalone worker process, so it can collide
    // with the API process's own SQLite writes (embed-marks). WAL serializes
    // writers; busy_timeout makes a blocked writer WAIT for the lock instead of
    // failing immediately (default 0 = fail-fast). Without this, a reindex pass
    // could intermittently error under cross-process write contention.
    db.pragma('busy_timeout = 5000');
    const results: Record<string, string> = {};
    const startTime = Date.now();

    const insert = db.prepare(`
      INSERT OR IGNORE INTO messages (source, source_id, channel, role, author, content, timestamp)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
    const updateWatermark = db.prepare(`
      INSERT INTO watermarks (source, last_seen, last_indexed) VALUES (?, ?, ?)
      ON CONFLICT(source) DO UPDATE SET last_seen = excluded.last_seen, last_indexed = excluded.last_indexed
    `);

    // #3067: prefer the injected positioned-tail reader; fall back to a
    // readFileSync slice (correct, but reads the whole file) for small logs / tests.
    const readTail = deps.readTail ?? ((p: string, max: number): string => {
      const full = deps.fs.readFileSync(p, 'utf-8');
      return full.length <= max ? full : full.slice(full.length - max);
    });
    // #3077 AC2: byte-offset incremental reader. Fallback reads the whole file then
    // slices by BYTE offset (correct for multibyte via Buffer) — fine for tests /
    // small logs; server.ts injects an fd positioned-read for the real 170MB log.
    const readSince = deps.readSince ?? ((p: string, offset: number) => {
      const full = deps.fs.readFileSync(p, 'utf-8');
      const buf = Buffer.from(full, 'utf-8');
      const size = buf.length;
      const start = offset > size ? 0 : offset;
      return { content: buf.subarray(start).toString('utf-8'), startOffset: start, size };
    });

    const ctx: IndexCtx = {
      db, insert, updateWatermark,
      fs: deps.fs, path: deps.path, repoRoot: deps.repoRoot, homedir: deps.homedir,
      now: nowFn(),
      readTail, readSince,
    };

    runSource('spine', results, () => indexSpine(ctx));
    runSource('briefs', results, () => indexBriefs(ctx));
    runSource('decisions', results, () => indexDecisions(ctx));
    runSource('adrs', results, () => indexAdrs(ctx));
    runSource('activity', results, () => indexActivity(ctx));
    runSource('memory', results, () => indexMemory(ctx));
    runSource('state', results, () => indexState(ctx));
    runSource('clearing', results, () => indexClearing(ctx));
    runSource('journal', results, () => indexJournal(ctx));
    runSource('stories', results, () => indexStories(ctx));
    clearSlackWatermarks(db, results);

    db.close();
    return { indexed: results, elapsed_ms: Date.now() - startTime };
  };
}
