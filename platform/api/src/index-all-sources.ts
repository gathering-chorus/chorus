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
  // #3136 REFINE — graph knowledge (principles/policies/practices) from Fuseki.
  // Prefetched ONCE (async) before the sync per-source loop, so the Fuseki round-trip
  // never runs inside runSource. Optional — omitted in tests → graph sources skip.
  fetchGraph?: () => Promise<Array<{ source: string; id: string; content: string }>>;
  // #3136 REFINE — full doc catalog (all ~402 docs from the SOURCE_DIRS scan, not the
  // 33-entry curated registry). Returns path-bearing entries; indexDocsFrom reads each
  // absPath's content. Injected so the indexer stays hermetic; tests stub it.
  listDocs?: () => Array<{ href: string; title?: string; group?: string; absPath?: string }>;
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

function runSource(name: string, results: Record<string, string>, fn: () => string | void): void {
  try {
    const summary = fn();
    if (summary) results[name] = summary;
  } catch (err: unknown) {
    results[name] = `error: ${err instanceof Error ? err.message : String(err)}`;
  }
}

// #3136 — spine REMOVED from the semantic index. Spine is telemetry: queried by
// trace_id / card / time via Loki (chorus.log → promtail), never by meaning. It was
// 82% of the corpus and the ~3s search-latency floor (1.1M rows to scan), and it
// drowned the durable knowledge that semantic search exists *for*. Verified nothing
// semantic-searches source=spine — context_inject reads chorus.log recent-N directly
// (query_recent_spine) — so removal costs no access path. The semantic index now
// holds only meaning-searchable content; telemetry stays in Loki.
//
// Neutered (not ripped out) as the minimal, non-cascading change that stops
// ingestion. The now-dead spine reader machinery (readTail / readSince deps,
// IndexEvent) + the server.ts injection are a flagged follow-up cleanup.
function indexSpine(_ctx: IndexCtx): string {
  return 'spine NOT indexed — telemetry lives in Loki (#3136)';
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
  if (!ctx.fs.existsSync(chatDir)) {
    // /tmp/chorus-chat is ephemeral — absent after every reboot. Checked-and-
    // empty is not dead: without this stamp the clearing watermark freezes at
    // its last pre-reboot value and freshness reports the source dead forever.
    ctx.updateWatermark.run!('clearing', ctx.now, ctx.now);
    return '0 transcripts indexed (chat dir absent)';
  }
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

// #3136 REFINE — doc-catalog. The 33 design docs in the catalog registry were
// never in the semantic index — searching "version-control-service-design" hit
// nothing. Index the rendered .html CONTENT (stripped to prose so matches land on
// the writing, not CSS/JS). filePaths may point at the sibling gathering repo or be
// absent on a given machine — existsSync-skip, same as every other indexer.
function htmlToText(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&[a-z]+;/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function indexDocsFrom(ctx: IndexCtx, docs: Array<{ href: string; title?: string; group?: string; absPath?: string }>): string {
  let indexed = 0;
  for (const doc of docs) {
    const p = doc.absPath;
    if (!p || !ctx.fs.existsSync(p)) continue;
    const text = htmlToText(String(ctx.fs.readFileSync(p, 'utf-8')));
    if (!text) continue;
    const ts = ctx.fs.statSync(p).mtime.toISOString();
    // Lead with title/href/group so a doc is findable by its name, not just its prose.
    const head = [doc.title, doc.href, doc.group].filter(Boolean).join(' — ');
    ctx.insert.run!('doc', `doc:${doc.href}`, 'doc-catalog', 'system', 'system', `${head}\n${text}`, ts);
    indexed++;
  }
  ctx.updateWatermark.run!('artifact:doc-catalog', ctx.now, ctx.now);
  return `${indexed} docs indexed`;
}

// #3136 REFINE — domains. The canonical product/domain/service model (tree.json):
// 7 products + 33 domains + services, each label+comment+owner+step+status+gaps.
// File-read — the graph's chorus:Domain count is 0, the tree IS the source. Answers
// "what is athena v2 / which domains exist / who owns X" — none of which the index
// could answer before.
// #3429 — safe stringify for unknown tree-node fields (no "[object Object]").
function str(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}
// #3429 — one tree node → its searchable content blob (extracted to keep
// indexDomains under the complexity threshold).
function nodeContent(node: Record<string, unknown>, kind: string, iri: string): string {
  const gaps = Array.isArray(node.gaps) ? (node.gaps as string[]).join('; ') : '';
  return [
    `${str(node.label) || iri} (${kind.replace(/s$/, '')})`,
    str(node.comment),
    node.ownedBy ? `owner: ${str(node.ownedBy)}` : '',
    node.atStep ? `step: ${str(node.atStep)}` : '',
    node.status ? `status: ${str(node.status)}` : '',
    gaps ? `gaps: ${gaps}` : '',
  ].filter(Boolean).join('\n');
}
function indexDomains(ctx: IndexCtx): string {
  const treePath = ctx.path.join(ctx.repoRoot, 'data/athena/tree.json');
  if (!ctx.fs.existsSync(treePath)) return '0 domains indexed (no tree)';
  const tree = JSON.parse(String(ctx.fs.readFileSync(treePath, 'utf-8'))) as Record<string, unknown>;
  let indexed = 0;
  for (const kind of ['products', 'domains', 'services']) {
    const arr = (tree[kind] as Array<Record<string, unknown>> | undefined) ?? [];
    for (const node of arr) {
      const iri = str(node.iri);
      if (!iri) continue;
      ctx.insert.run!('domain', `domain:${iri}`, 'athena-tree', 'system', 'system', nodeContent(node, kind, iri), ctx.now);
      indexed++;
    }
  }
  ctx.updateWatermark.run!('artifact:domains', ctx.now, ctx.now);
  return `${indexed} domains indexed`;
}

// #3136 REFINE — graph knowledge. principles (28) / policies (14) / practices (40)
// live in Fuseki and were never semantically searchable. Rows are prefetched once
// (deps.fetchGraph) so the Fuseki round-trip stays out of the sync loop; here we
// just insert the in-memory rows for one source. Searching "ship small" or
// "actor-bdd" now reaches the principle / practice, not 8 claude echoes.
function indexGraphRows(ctx: IndexCtx, rows: Array<{ source: string; id: string; content: string }>, source: string): string {
  const subset = rows.filter((r) => r.source === source);
  for (const r of subset) {
    ctx.insert.run!(source, r.id, `loom:${source}`, 'system', 'system', r.content, ctx.now);
  }
  ctx.updateWatermark.run!(`graph:${source}`, ctx.now, ctx.now);
  return `${subset.length} ${source} indexed`;
}

// #3136 — coverage signal. The failure this card exists to prevent is a knowledge
// source silently ingesting nothing (the invisible-canon bug). After every reindex,
// flag any source that SHOULD be present but came back absent or zero. Minimal: the
// per-source summaries already carry leading counts — parse + check, warn on miss.
// Not a new processing layer; a guard on the existing return value.
function coverageCheck(results: Record<string, string>, expected: string[]): string {
  const missing: string[] = [];
  for (const src of expected) {
    const summary = results[src];
    if (!summary) { missing.push(`${src}=absent`); continue; }
    const n = parseInt(summary, 10);
    if (Number.isFinite(n) && n === 0) missing.push(`${src}=0`);
  }
  if (missing.length) {
    const msg = `WARN coverage — knowledge source(s) empty: ${missing.join(', ')}`;
     
    console.warn(`[index-all-sources] ${msg}`);
    return msg;
  }
  return `ok (${expected.length} knowledge sources present)`;
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
    const docList = deps.listDocs ? deps.listDocs() : [];
    runSource('docs', results, () => indexDocsFrom(ctx, docList));
    runSource('domains', results, () => indexDomains(ctx));

    // #3136 REFINE — prefetch graph knowledge once (async). Fuseki-down must not tank
    // the file-source indexers, so isolate in its own try and fall back to empty (the
    // coverage signal below then flags principles/policies/practices=0).
    let graphRows: Array<{ source: string; id: string; content: string }> = [];
    if (deps.fetchGraph) {
      try { graphRows = await deps.fetchGraph(); }
      catch (err: unknown) { results.graph = `error: ${err instanceof Error ? err.message : String(err)}`; }
    }
    runSource('principles', results, () => indexGraphRows(ctx, graphRows, 'principle'));
    runSource('policies', results, () => indexGraphRows(ctx, graphRows, 'policy'));
    runSource('practices', results, () => indexGraphRows(ctx, graphRows, 'practice'));
    clearSlackWatermarks(db, results);

    // #3136 coverage signal — flag any knowledge source that came back empty.
    const expected = ['briefs', 'decisions', 'adrs', 'memory', 'docs', 'domains'];
    if (deps.fetchGraph) expected.push('principles', 'policies', 'practices');
    results._coverage = coverageCheck(results, expected);

    db.close();
    return { indexed: results, elapsed_ms: Date.now() - startTime };
  };
}
