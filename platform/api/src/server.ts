import express, { Request, Response, NextFunction } from 'express';
import Database from 'better-sqlite3';
import { execFile, exec } from 'child_process';
import { promisify } from 'util';
import path from 'path';
import os from 'os';
import fs from 'fs';
import * as lancedb from '@lancedb/lancedb';

const execAsync = promisify(exec);
const CHORUS_ROOT = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects';

const app = express();
app.use(express.json());

import { getHooksSummary } from './hooks-summary';
import { getCostSummary } from './cost-summary';
import { getFitnessSummary } from './fitness-summary';
import { getQualityScan, getQualityByDomain } from './quality-summary';
import { getPatternsSummary } from './patterns-summary';
import { getPostureStrip, getWerkActivity } from './jeff-summary';
import { listSessions, getSession, getSessionLog, isValidSessionId } from './session-replay';

// Serve Chorus landing at root — #2099 (promoted from /docs per product feedback)
app.use('/', express.static(path.join(__dirname, '..', 'public')));

// Legacy alias — /docs predates the landing's promotion to / (#2108). Remove once clients migrated.
app.use('/docs', express.static(path.join(__dirname, '..', 'public')));

// Serve Borg shaping surface — #2099
app.use('/borg', express.static(path.join(__dirname, '..', 'public', 'borg')));

// Borg — Hooks summary endpoint — #2099
// Borg summary delegates — #2173 AC4: uniform run() wrapper replaces
// per-handler try/catch boilerplate. Each adapter is one line.
import { run } from './handlers/util';

app.get('/api/chorus/hooks/summary', async (_req, res) => {
  const r = await run(() => getHooksSummary());
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/cost/summary', async (_req, res) => {
  const r = await run(() => getCostSummary());
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/fitness/summary', async (_req, res) => {
  const r = await run(() => getFitnessSummary());
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/quality/summary', async (_req, res) => {
  const r = await run(() => getQualityScan());
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/quality/domain/:domain', async (req, res) => {
  const domain = String(req.params.domain || '').toLowerCase();
  const r = await run(() => getQualityByDomain(domain));
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/patterns/summary', async (req, res) => {
  const days = parseInt(String(req.query.days || '30'), 10) || 30;
  const r = await run(() => getPatternsSummary(days));
  res.status(r.status).json(r.body);
});

// Borg — codebase topology proxy (Gathering owns the RDF source) — #2099
// Extracted to handlers/codebase-topology.ts (#2173 AC4). The adapter is the
// uniform shape for all extracted handlers: call the pure fn, map its
// {status, body} to res.status().json(). No try/catch — the pure fn already
// maps throws to {status: 502, body: {error}}.
import { fetchTopology } from './handlers/codebase-topology';
app.get('/api/chorus/codebase/topology', async (_req, res) => {
  const r = await fetchTopology();
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/jeff/posture/strip', async (req, res) => {
  const days = parseInt(String(req.query.days || '7'), 10) || 7;
  const posture = String(req.query.posture || 'all');
  const mood = String(req.query.mood || 'all');
  const r = await run(() => getPostureStrip(days, posture, mood));
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/werk/activity', async (req, res) => {
  const hours = parseInt(String(req.query.hours || '24'), 10) || 24;
  const role = String(req.query.role || '');
  const event = String(req.query.event || '');
  const r = await run(() => getWerkActivity(hours, role, event));
  res.status(r.status).json(r.body);
});

// Borg — Session replay: list — #2099
// Extracted to handlers/sessions.ts (#2173 AC4). Three handlers share a deps
// object bound to session-replay.ts. The log endpoint uses the contentType
// field on FetchResult — default json applies everywhere else.
import {
  fetchSessionList,
  fetchSessionById,
  fetchSessionLog,
} from './handlers/sessions';

const sessionDeps = { listSessions, getSession, getSessionLog, isValidSessionId };

app.get('/api/chorus/sessions', (_req, res) => {
  const r = fetchSessionList(sessionDeps);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/sessions/:id', (req, res) => {
  const r = fetchSessionById(sessionDeps, req.params.id);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/sessions/:id/log', (req, res) => {
  const r = fetchSessionLog(sessionDeps, req.params.id);
  if (r.contentType === 'text/plain') {
    res.status(r.status).type('text/plain').send(r.body);
    return;
  }
  res.status(r.status).json(r.body);
});

// Request logging — every request writes to stdout so the log stays fresh
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => {
    const ms = Date.now() - start;
    console.log(`[chorus-api] ${req.method} ${req.path} ${res.statusCode} ${ms}ms`);
  });
  next();
});
const PORT = parseInt(process.env.CHORUS_API_PORT || '3340', 10);
const DB_PATH = path.join(os.homedir(), '.chorus', 'index.db');
const LANCE_DIR = path.join(os.homedir(), '.chorus', 'lance');
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';
const EMBED_MODEL = 'nomic-embed-text';
// Prefer repo scripts (always present), fall back to ~/.chorus/scripts
const REPO_ROOT = path.resolve(__dirname, '../../..');
const REPO_SCRIPTS = path.resolve(__dirname, '../../scripts');
const HOME_SCRIPTS = path.join(os.homedir(), '.chorus', 'scripts');
const SCRIPTS_DIR = fs.existsSync(REPO_SCRIPTS) ? REPO_SCRIPTS : HOME_SCRIPTS;

// --- Board card cache (#2096) ---
// Same cache pattern as health endpoint (#1978, Silas: 1566ms→2ms with 30s cache).
// Log evidence: /api/chorus/domain/:name = 344-536ms, all other facets = 8-29ms.
// Root cause: execAsync('cards list') shells out on every request.
// Both /api/chorus/domain/:name and /api/athena/subdomains/:id/cards read from cache.

interface CachedCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  type: string;
  priority: string;
  tags: string;
}

let boardCache: CachedCard[] = [];
let boardCacheAge = 0;

async function refreshBoardCache(): Promise<void> {
  try {
    const boardTs = path.join(REPO_SCRIPTS, 'cards');
    const envOpts = {
      encoding: 'utf-8' as const, timeout: 15000,
      env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
    };
    const { stdout } = await execAsync(`bash ${boardTs} list 2>/dev/null`, envOpts);
    const cards: CachedCard[] = [];
    let currentStatus = '';
    for (const line of stdout.split('\n')) {
      const statusMatch = line.match(/^(WIP|Blocked|Now|Next|Later|Done|Won't Do)\s*\(\d+\)/);
      if (statusMatch) { currentStatus = statusMatch[1]; continue; }
      const cardMatch = line.trim().match(/^(\d+)\s+(.+?)\s+\[([^\]]+)\]$/);
      if (cardMatch) {
        const tags = cardMatch[3];
        const ownerMatch = tags.match(/^(Wren|Silas|Kade|Jeff)/i);
        const typeMatch = tags.match(/type:(\w+)/);
        const priorityMatch = tags.match(/P([1-3])/);
        cards.push({
          id: cardMatch[1],
          title: cardMatch[2].trim(),
          status: currentStatus,
          owner: ownerMatch ? ownerMatch[1].toLowerCase() : '',
          type: typeMatch ? typeMatch[1] : '',
          priority: priorityMatch ? priorityMatch[0] : '',
          tags,
        });
      }
    }
    boardCache = cards;
    boardCacheAge = Date.now();
  } catch (err) {
    console.error(`[chorus-api] board cache refresh failed: ${(err as Error).message}`);
  }
}

function getBoardCards(): CachedCard[] {
  if (boardCache.length === 0 && boardCacheAge === 0) {
    refreshBoardCache();
  }
  return boardCache;
}

refreshBoardCache();
setInterval(refreshBoardCache, 60_000);

// --- LanceDB semantic search ---

let lanceTable: lancedb.Table | null = null;
let lanceDb: lancedb.Connection | null = null;

async function initLance(): Promise<void> {
  if (!fs.existsSync(LANCE_DIR)) return;
  try {
    lanceDb = await lancedb.connect(LANCE_DIR);
    const tables = await lanceDb.tableNames();
    if (tables.includes('messages')) {
      lanceTable = await lanceDb.openTable('messages');
      const count = await lanceTable.countRows();
      console.log(`[chorus-api] LanceDB: ${count} vectors loaded`);
    }
  } catch (err) {
    console.error(`[chorus-api] LanceDB init failed (non-fatal): ${err}`);
  }
}

// --- Embed-at-ingest: embed new messages after indexing ---

const MIN_EMBED_LENGTH = 100;
const EMBED_PAGE_SIZE = 100;  // Process one page per cycle, timer handles the rest (#1920)

async function embedDelta(): Promise<{ embedded: number; skipped: number; ollama_failures: number }> {
  if (!lanceDb) {
    await initLance();
    if (!lanceDb) return { embedded: 0, skipped: 0, ollama_failures: 0 };
  }

  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');

  try {
    // Track embedded state in SQLite — add column if missing (#1920)
    // Using a writable connection for the schema check only
    const rwDb = new Database(DB_PATH);
    rwDb.pragma('journal_mode = WAL');
    try {
      rwDb.exec(`ALTER TABLE messages ADD COLUMN embedded INTEGER DEFAULT 0`);
    } catch { /* column already exists */ }
    rwDb.close();

    // Page through unembedded messages — one page per call (#1920)
    const page = db.prepare(`
      SELECT id, source, channel, role, content, timestamp
      FROM messages
      WHERE embedded = 0 AND LENGTH(content) >= ?
      ORDER BY id ASC
      LIMIT ?
    `).all(MIN_EMBED_LENGTH, EMBED_PAGE_SIZE) as Array<{
      id: number; source: string; channel: string; role: string; content: string; timestamp: string;
    }>;

    if (page.length === 0) return { embedded: 0, skipped: 0, ollama_failures: 0 };

    // Count total remaining for logging
    const countRow = db.prepare(`
      SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= ?
    `).get(MIN_EMBED_LENGTH) as { cnt: number };

    const records: Array<{
      msg_id: number; source: string; channel: string; role: string;
      content: string; timestamp: string; vector: number[];
    }> = [];
    let skipped = 0;

    let ollamaFailures = 0;
    for (const msg of page) {
      try {
        const text = `[${msg.source}/${msg.role}] ${msg.content.slice(0, 2000)}`;
        const vector = await embedQuery(text);
        records.push({
          msg_id: msg.id,
          source: msg.source,
          channel: msg.channel,
          role: msg.role,
          content: msg.content.slice(0, 2000),
          timestamp: msg.timestamp,
          vector,
        });
      } catch (err: any) {
        skipped++;
        ollamaFailures++;
        console.error(`[embed-delta] Ollama failure for msg ${msg.id}: ${err.message}`);
      }
    }

    if (records.length === 0) return { embedded: 0, skipped, ollama_failures: ollamaFailures };

    // Write page to LanceDB — incremental, restartable
    if (lanceTable) {
      await lanceTable.add(records);
    } else if (lanceDb) {
      lanceTable = await lanceDb.createTable('messages', records);
    }

    // Mark as embedded in SQLite so we don't reprocess (#1920)
    const markDb = new Database(DB_PATH);
    markDb.pragma('journal_mode = WAL');
    const markStmt = markDb.prepare(`UPDATE messages SET embedded = 1 WHERE id = ?`);
    const markMany = markDb.transaction((ids: number[]) => {
      for (const id of ids) markStmt.run(id);
    });
    markMany(records.map(r => r.msg_id));
    markDb.close();

    if (ollamaFailures > 0) {
      console.log(`[embed-delta] Embedded ${records.length}/${countRow.cnt} remaining, skipped ${skipped}, ollama_failures ${ollamaFailures}`);
    } else {
      console.log(`[embed-delta] Embedded ${records.length}/${countRow.cnt} remaining, skipped ${skipped}`);
    }
    return { embedded: records.length, skipped, ollama_failures: ollamaFailures };
  } finally {
    db.close();
  }
}

const EMBED_MAX_RETRIES = 3;
const EMBED_BACKOFF_MS = [1000, 2000, 4000];

// #2168 AC-14: LRU cache for query embeddings. Ollama embed is ~1s per call;
// envelope queries repeat or have minor variants. 128 entries / 10min TTL.
const EMBED_CACHE_MAX = 128;
const EMBED_CACHE_TTL_MS = 10 * 60 * 1000;
const embedCache = new Map<string, { vec: number[]; ts: number }>();

async function embedQuery(text: string): Promise<number[]> {
  const key = text.trim().toLowerCase();
  const now = Date.now();
  const hit = embedCache.get(key);
  if (hit && now - hit.ts < EMBED_CACHE_TTL_MS) {
    // Touch (LRU): delete and re-set
    embedCache.delete(key);
    embedCache.set(key, hit);
    return hit.vec;
  }

  let lastErr: Error | null = null;
  for (let attempt = 0; attempt < EMBED_MAX_RETRIES; attempt++) {
    try {
      const res = await fetch(`${OLLAMA_URL}/api/embeddings`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: EMBED_MODEL, prompt: text }),
        signal: AbortSignal.timeout(15000),
      });
      if (!res.ok) throw new Error(`Ollama embed failed: ${res.status}`);
      const data = await res.json() as { embedding: number[] };
      // Cache + evict oldest if over cap
      embedCache.set(key, { vec: data.embedding, ts: now });
      if (embedCache.size > EMBED_CACHE_MAX) {
        const oldest = embedCache.keys().next().value;
        if (oldest) embedCache.delete(oldest);
      }
      return data.embedding;
    } catch (err: any) {
      lastErr = err;
      if (attempt < EMBED_MAX_RETRIES - 1) {
        await new Promise(r => setTimeout(r, EMBED_BACKOFF_MS[attempt]));
      }
    }
  }
  throw lastErr || new Error('Ollama embed failed after retries');
}

interface SemanticResult {
  msg_id: number;
  source: string;
  channel: string;
  role: string;
  content: string;
  timestamp: string;
  score: number;
}

async function semanticSearch(query: string, limit: number, role?: string): Promise<SemanticResult[]> {
  if (!lanceTable) return [];
  const queryVec = await embedQuery(query);
  let builder = lanceTable.vectorSearch(queryVec).limit(limit * 2); // over-fetch for filtering
  const results = await builder.toArray();

  let filtered = results;
  if (role) {
    filtered = results.filter((r: any) => r.role === role);
  }

  return filtered.slice(0, limit).map((r: any) => ({
    msg_id: r.msg_id,
    source: r.source || '',
    channel: r.channel || '',
    role: r.role || '',
    content: r.content || '',
    timestamp: r.timestamp || '',
    score: r._distance != null ? 1 / (1 + r._distance) : 0,
  }));
}
const STALE_THRESHOLD_MS = 60 * 60 * 1000; // 1 hour
const FUSEKI_URL = process.env.FUSEKI_URL || 'http://localhost:3030/pods/query';

// --- SPARQL text search ---

interface SparqlResult {
  uri: string;
  type: string;
  domain: string;
  label: string;
  content: string;
  score: number;
}

async function sparqlSearch(query: string, limit: number): Promise<SparqlResult[]> {
  const terms = query.split(/\s+/).filter(t => t.length > 2);
  if (terms.length === 0) return [];

  // Build FILTER with CONTAINS for each term (case-insensitive via LCASE)
  // AND for multi-term: require all terms present (tighter relevance)
  const filters = terms.map((_, i) => `CONTAINS(LCASE(?text), LCASE(?term${i}))`).join(' && ');
  const binds = terms.map((t, i) => `BIND("${t.replace(/"/g, '\\"')}" AS ?term${i})`).join('\n    ');

  const sparql = `
    PREFIX jb: <https://jeffbridwell.com/ontology#>
    PREFIX dcterms: <http://purl.org/dc/terms/>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    PREFIX schema: <https://schema.org/>

    SELECT DISTINCT ?s ?type ?domain ?label ?text WHERE {
      GRAPH ?g {
        ?s a ?type .
        { ?s dcterms:title ?label } UNION { ?s rdfs:label ?label } UNION { ?s schema:name ?label }
        OPTIONAL { ?s dcterms:description ?desc }
        BIND(COALESCE(CONCAT(STR(?label), " ", COALESCE(STR(?desc), "")), STR(?label)) AS ?text)
        ${binds}
        FILTER(${filters})
      }
      BIND(REPLACE(STR(?g), "http://localhost:3000/pods/jeff/([^/]+)/.*", "$1") AS ?domain)
    }
    LIMIT ${Math.min(limit, 50)}
  `;

  try {
    const res = await fetch(`${FUSEKI_URL}?query=${encodeURIComponent(sparql)}`, {
      headers: { 'Accept': 'application/sparql-results+json' },
      signal: AbortSignal.timeout(10000),
    });
    if (!res.ok) return [];
    const data = await res.json() as { results: { bindings: any[] } };

    return data.results.bindings.map((b: any) => ({
      uri: b.s?.value || '',
      type: (b.type?.value || '').replace(/.*[#/]/, ''),
      domain: b.domain?.value || '',
      label: b.label?.value || '',
      content: b.text?.value || b.label?.value || '',
      score: 0.5, // baseline score for RRF merge
    }));
  } catch {
    return [];
  }
}

// --- Unified search: merge all sources via RRF ---

interface UnifiedResult {
  id?: number;
  uri?: string;
  source: string;
  type?: string;
  domain?: string;
  role?: string;
  content: string;
  timestamp?: string;
  label?: string;
  _rrf_score: number;
  _sources: string[];
}

function mergeUnified(
  ftsResults: any[],
  semanticResults: SemanticResult[],
  sparqlResults: SparqlResult[],
  limit: number,
  k: number = 60
): UnifiedResult[] {
  const scoreMap = new Map<string, UnifiedResult>();

  // Score FTS results
  ftsResults.forEach((r, i) => {
    const key = `chorus:${r.id}`;
    const entry = scoreMap.get(key) || {
      id: r.id, source: r.source || 'chorus', role: r.role, content: r.content,
      timestamp: r.timestamp, _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('fts')) entry._sources.push('fts');
    scoreMap.set(key, entry);
  });

  // Score semantic results
  semanticResults.forEach((r, i) => {
    const key = `chorus:${r.msg_id}`;
    const entry = scoreMap.get(key) || {
      id: r.msg_id, source: r.source || 'chorus', role: r.role, content: r.content,
      timestamp: r.timestamp, _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('semantic')) entry._sources.push('semantic');
    scoreMap.set(key, entry);
  });

  // Score SPARQL results
  sparqlResults.forEach((r, i) => {
    const key = `sparql:${r.uri}`;
    const entry = scoreMap.get(key) || {
      uri: r.uri, source: 'sparql', type: r.type, domain: r.domain,
      label: r.label, content: r.content,
      _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('sparql')) entry._sources.push('sparql');
    scoreMap.set(key, entry);
  });

  return Array.from(scoreMap.values())
    .sort((a, b) => b._rrf_score - a._rrf_score)
    .slice(0, limit);
}

// --- Spine event emitter (fire-and-forget to chorus-log.sh) ---
const CHORUS_LOG = path.join(process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus'), 'platform/scripts/chorus-log');

function emitSearchEvent(fields: Record<string, string | number>): void {
  const args = ['search.query.executed', 'system', ...Object.entries(fields).map(([k, v]) => `${k}=${v}`)];
  execFile(CHORUS_LOG, args, { timeout: 5000 }, () => {});
}

// --- Database helper ---

function getDb(): Database.Database {
  if (!fs.existsSync(DB_PATH)) {
    throw new DbNotFoundError();
  }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  return db;
}

class DbNotFoundError extends Error {
  constructor() { super('Chorus index database not found'); }
}

// --- Staleness check middleware ---

function addStaleHeader(res: Response, db: Database.Database): void {
  const row = db.prepare(
    `SELECT MAX(last_indexed) as latest FROM watermarks`
  ).get() as { latest: string } | undefined;

  if (row?.latest) {
    const lastIndexed = new Date(row.latest).getTime();
    if (Date.now() - lastIndexed > STALE_THRESHOLD_MS) {
      res.setHeader('X-Chorus-Stale', 'true');
    }
  }
}

// --- Search freshness metadata (#1878) ---

function buildSearchMeta(results: any[], db?: Database.Database): Record<string, any> {
  // Coverage: proportion of indexed sources within 2x their expected cadence (#1879)
  let domain_coverage = 1;
  if (db) {
    try {
      const watermarks = db.prepare('SELECT source, last_indexed FROM watermarks ORDER BY source').all() as Array<{ source: string; last_indexed: string }>;
      const aggregated = new Map<string, string>();
      for (const w of watermarks) {
        const parts = w.source.split(':');
        const key = parts[0] === 'artifact' ? parts.slice(0, 2).join(':') : parts[0];
        const existing = aggregated.get(key);
        if (!existing || w.last_indexed > existing) aggregated.set(key, w.last_indexed);
      }
      const now = Date.now();
      let total = 0, contributing = 0;
      for (const [source, lastIndexed] of aggregated) {
        total++;
        const ageSecs = (now - new Date(lastIndexed).getTime()) / 1000;
        const cadenceKey = source.split(':')[0];
        const cadence = SOURCE_CADENCE[cadenceKey] || SOURCE_CADENCE[source] || 86400;
        if (ageSecs / cadence <= 2) contributing++;
      }
      domain_coverage = total > 0 ? contributing / total : 1;
    } catch { /* default to 1 */ }
  }

  let newest_result_age_s = 0;
  if (results.length > 0) {
    const timestamps = results
      .map((r: any) => r.timestamp)
      .filter(Boolean)
      .map((t: string) => new Date(t).getTime())
      .filter((t: number) => !isNaN(t));
    if (timestamps.length > 0) {
      const newest = Math.max(...timestamps);
      newest_result_age_s = Math.round((Date.now() - newest) / 1000);
    }
  }

  const stale = newest_result_age_s > 86400 || domain_coverage < 0.5;

  const sources: Record<string, number> = {};
  for (const r of results) {
    const src = r.source || r.domain || 'unknown';
    sources[src] = (sources[src] || 0) + 1;
  }

  return { domain_coverage: Math.round(domain_coverage * 100) / 100, newest_result_age_s, stale, sources, schema_version: '1.0.0' };
}

// #2174 AC-1: per-hit freshness. Structured so agents don't re-parse content
// for timestamp (AC-2 semantic).
function enrichHit(r: any, now: number): any {
  const ts = r?.timestamp;
  let freshness_s = 0;
  if (ts) {
    const t = new Date(ts).getTime();
    if (!isNaN(t)) freshness_s = Math.max(0, Math.round((now - t) / 1000));
  }
  return { ...r, freshness_s };
}

// #2174 AC-6: small default limit for token economy; caller override up to 100.
const SEARCH_DEFAULT_LIMIT = 5;
const SEARCH_MAX_LIMIT = 100;
function resolveSearchLimit(raw: string | undefined): { limit: number; explicit: boolean } {
  if (raw === undefined) return { limit: SEARCH_DEFAULT_LIMIT, explicit: false };
  const n = parseInt(raw, 10);
  if (isNaN(n) || n < 1) return { limit: SEARCH_DEFAULT_LIMIT, explicit: false };
  return { limit: Math.min(n, SEARCH_MAX_LIMIT), explicit: true };
}

// --- GET /api/chorus/search ---
// Supports mode=fts (default), mode=semantic, mode=hybrid

import { fetchSearch } from './handlers/chorus-search';
app.get('/api/chorus/search', async (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    addStaleHeader(res, db);
    const r = await fetchSearch(
      {
        db,
        semanticSearch: lanceTable ? (semanticSearch as unknown as import('./handlers/chorus-search').SemanticSearchFn) : undefined,
        sparqlSearch: sparqlSearch as unknown as import('./handlers/chorus-search').SparqlSearchFn,
        mergeUnified: mergeUnified as unknown as import('./handlers/chorus-search').MergeUnifiedFn,
        mergeRRF: mergeRRF as unknown as import('./handlers/chorus-search').MergeRRFFn,
        emitSearchEvent,
        buildSearchMeta,
        enrichHit,
        resolveSearchLimit,
      },
      {
        q: req.query.q as string | undefined,
        limit: req.query.limit as string | undefined,
        role: req.query.role as string | undefined,
        mode: req.query.mode as string | undefined,
      },
    );
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/conversation ---
// Returns a readable conversation thread between participants in a time range.
// Memory domain — team recall, not search. #1946

import { fetchChorusConversation } from './handlers/chorus-conversation';
app.get('/api/chorus/conversation', (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    const r = fetchChorusConversation(
      { db, isEDT, convertToLocal },
      {
        roles: req.query.roles as string | undefined,
        date: req.query.date as string | undefined,
        tz: req.query.tz as string | undefined,
        after: req.query.after as string | undefined,
        before: req.query.before as string | undefined,
        limit: req.query.limit as string | undefined,
      },
    );
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/card-story/:id ---
// Memory domain — join six data sources into a card timeline. #1947

import { fetchChorusCardStory, type CardMeta, type NudgeMessage } from './handlers/chorus-card-story';
app.get('/api/chorus/card-story/:id', async (req: Request, res: Response) => {
  const cardsScript = path.resolve(__dirname, '../../scripts/cards');
  const MESSAGING_URL = 'http://localhost:3475';
  const logPath = path.resolve(__dirname, '../../logs/chorus.log');

  let db: Database.Database | null = null;
  try { db = getDb(); } catch { /* db optional */ }

  try {
    const r = await fetchChorusCardStory(
      {
        loadCard: async (cardId) => {
          const { stdout } = await execAsync(
            `bash ${cardsScript} view ${cardId} --json 2>/dev/null`,
            { encoding: 'utf-8', timeout: 10000, env: { ...process.env, PATH: `/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } },
          );
          return JSON.parse(stdout) as CardMeta;
        },
        db,
        readLog: () => fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf-8') : null,
        loadNudges: async () => {
          const resp = await fetch(`${MESSAGING_URL}/api/messages?limit=100`);
          if (!resp.ok) return [];
          return (await resp.json()) as NudgeMessage[];
        },
      },
      req.params.id,
    );
    res.status(r.status).json(r.body);
  } finally {
    if (db) db.close();
  }
});

// --- GET /api/chorus/domain-story/:domain ---
// Memory domain — institutional memory for a domain. Cards + conversation mentions + spine events. #1947

import { fetchChorusDomainStory } from './handlers/chorus-domain-story';
app.get('/api/chorus/domain-story/:domain', async (req: Request, res: Response) => {
  const logPath = path.resolve(__dirname, '../../logs/chorus.log');
  let db: Database.Database | null = null;
  try { db = getDb(); } catch { /* db optional */ }
  try {
    const r = fetchChorusDomainStory(
      {
        getCards: () => getBoardCards(),
        db,
        readLog: () => fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf-8') : null,
      },
      req.params.domain,
      req.query.limit as string | undefined,
    );
    res.status(r.status).json(r.body);
  } finally {
    if (db) db.close();
  }
});

// --- GET /api/chorus/crawl/:domain ---
// Memory domain — domain crawler. Traverse OWL + cards + conversations + spine into connected subgraph. #1956

app.get('/api/chorus/crawl/:domain', async (req: Request, res: Response) => {
  const domain = req.params.domain.toLowerCase();

  // Domain validation — return 404 for unknown domains (#1886, #2026)
  // #2026: Use knownDomains list, not subdomain ontology. "search" and "infrastructure"
  // are valid crawl targets even without ontology subdomains — they have cards, code, logs.
  const knownCrawlDomains = ['photos', 'music', 'people', 'books', 'cooking', 'reading', 'watching', 'property', 'stories', 'notes', 'blog', 'gallery', 'social', 'glimmers', 'ideas', 'seeds', 'self', 'chorus', 'clearing', 'pulse', 'spine', 'interactions', 'memory', 'infrastructure', 'observability', 'loom', 'search'];
  if (!knownCrawlDomains.includes(domain)) {
    res.status(404).json({
      error: `Domain '${domain}' not found`,
      suggestion: 'Valid domains: ' + knownCrawlDomains.join(', '),
      valid_count: knownCrawlDomains.length,
    });
    return;
  }

  const FUSEKI_URL = 'http://localhost:3030';
  const NODE_PATH = '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin';

  const cards: any[] = [];
  const rdf: { classes: string[]; instances: number; count: number; relationships: string[] } = { classes: [], instances: 0, count: 0, relationships: [] };
  const owl: { properties: string[]; relationships: string[] } = { properties: [], relationships: [] };
  const mentions: any[] = [];
  const spine: any[] = [];
  const code: { files: string[] } = { files: [] };
  const infra: { launchagents: string[]; endpoints: string[]; monitoring: string[] } = { launchagents: [], endpoints: [], monitoring: [] };
  const links: any[] = [];
  const related: any[] = [];
  const history: { unresolved: any[]; feedback: string[]; trust_score: number; health: string } = { unresolved: [], feedback: [], trust_score: 0, health: '' };
  const timeline: any[] = [];

  // Source 1: Cards tagged with this domain — #2096: read from board cache
  {
    const allCached = getBoardCards();
    const domainCounts: Record<string, number> = {};

    // Build domain counts from all cards for related calculation
    for (const c of allCached) {
      const dm = c.tags.match(/domain:(\w+)/);
      if (dm) domainCounts[dm[1]] = (domainCounts[dm[1]] || 0) + 1;
    }

    for (const c of allCached.filter(c => c.tags.includes(`domain:${domain}`))) {
      const cardIndex = parseInt(c.id, 10);
      const card = {
        index: cardIndex,
        title: c.title,
        status: c.status,
        owner: c.owner,
      };
      cards.push(card);
      timeline.push({ timestamp: '', source: 'card', text: `#${c.id} ${c.title} [${c.status}]`, role: c.owner, card: cardIndex });

      if (c.status !== 'Done' && c.status !== "Won't Do") {
        history.unresolved.push(card);
      }
    }

    // Related domains: any domain that shares conversation mentions
    // (calculated after mentions phase below)
  }

  // Source 2: Fuseki RDF triples
  try {
    const sparql = `SELECT ?s ?p ?o WHERE { GRAPH ?g { ?s ?p ?o } FILTER(CONTAINS(LCASE(STR(?g)), '${domain}')) } LIMIT 200`;
    const fusekiResp = await fetch(`${FUSEKI_URL}/gathering/sparql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json' },
      body: `query=${encodeURIComponent(sparql)}`,
    });
    if (fusekiResp.ok) {
      const result = await fusekiResp.json() as any;
      const bindings = result.results?.bindings || [];
      rdf.count = bindings.length;

      // Extract unique classes (objects of rdf:type predicates)
      const classes = new Set<string>();
      for (const b of bindings) {
        if (b.p?.value?.includes('type') || b.p?.value?.includes('Type')) {
          classes.add(b.o?.value || '');
        }
      }
      rdf.classes = [...classes].filter(c => c.length > 0);
      rdf.instances = bindings.length;
    }
  } catch { /* fuseki not available */ }

  // Source 3: Session mentions from Chorus index
  let db: Database.Database | null = null;
  try {
    db = getDb();
    const rows = db.prepare(`
      SELECT author, content, timestamp, role
      FROM messages
      WHERE content LIKE ?
      ORDER BY timestamp ASC
      LIMIT 100
    `).all(`%${domain}%`) as Array<{ author: string; content: string; timestamp: string; role: string }>;

    const relatedDomainCounts: Record<string, number> = {};

    for (const m of rows) {
      const text = m.content.trim();
      if (text.startsWith('<system-reminder>')) continue;
      if (text.startsWith('Base directory for this skill:')) continue;
      if (text.length < 20) continue;

      const speaker = m.author === 'user' ? 'jeff' : m.role;
      mentions.push({ timestamp: m.timestamp, role: speaker, text: text.slice(0, 300) });
      timeline.push({ timestamp: m.timestamp, source: 'chorus-index', text: text.slice(0, 300), role: speaker });

      // Find other domains mentioned in same conversations
      const domainRefs = text.match(/domain:(\w+)/g) || [];
      for (const ref of domainRefs) {
        const d = ref.replace('domain:', '');
        if (d !== domain) {
          relatedDomainCounts[d] = (relatedDomainCounts[d] || 0) + 1;
        }
      }

      // Check for cross-domain references in conversation text
      const knownDomains = ['photos', 'music', 'people', 'books', 'cooking', 'reading', 'watching', 'property', 'stories', 'notes', 'blog', 'gallery', 'social', 'glimmers', 'ideas', 'seeds', 'self', 'chorus', 'clearing', 'pulse', 'spine', 'interactions', 'memory', 'infrastructure', 'observability', 'loom', 'search'];
      const lower = text.toLowerCase();
      for (const kd of knownDomains) {
        if (kd !== domain && lower.includes(kd)) {
          relatedDomainCounts[kd] = (relatedDomainCounts[kd] || 0) + 1;
        }
      }
    }

    // Build related domains list
    for (const [d, count] of Object.entries(relatedDomainCounts)) {
      related.push({ domain: d, strength: count });
    }
    related.sort((a, b) => b.strength - a.strength);

  } catch { /* db not available */ }
  finally { if (db) db.close(); }

  // Source 4: Spine events from chorus.log
  try {
    const logPath = path.resolve(__dirname, '../../logs/chorus.log');
    if (fs.existsSync(logPath)) {
      const cardIds = new Set(cards.map(c => c.index));
      const lines = fs.readFileSync(logPath, 'utf-8').split('\n');
      for (const line of lines) {
        try {
          const parsed = JSON.parse(line);
          if (!parsed.event || !parsed.event.startsWith('card.')) continue;
          const cardId = parseInt(parsed.card || '0', 10);
          if (!cardIds.has(cardId)) continue;
          spine.push({ timestamp: parsed.timestamp, event: parsed.event, role: parsed.role, card: cardId });
          timeline.push({ timestamp: parsed.timestamp, source: 'spine', text: parsed.event, role: parsed.role, card: cardId });
        } catch { /* skip */ }
      }
    }
  } catch { /* log not readable */ }

  // Source 5: OWL classes from Fuseki ontology
  try {
    const owlSparql = `
      PREFIX owl: <http://www.w3.org/2002/07/owl#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      PREFIX chorus: <urn:chorus:ontology/>
      SELECT ?class ?prop ?range WHERE {
        ?class a owl:Class .
        FILTER(CONTAINS(LCASE(STR(?class)), '${domain}'))
        OPTIONAL { ?prop rdfs:domain ?class . ?prop rdfs:range ?range . }
      } LIMIT 50
    `;
    const owlResp = await fetch(`${FUSEKI_URL}/gathering/sparql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/sparql-results+json' },
      body: `query=${encodeURIComponent(owlSparql)}`,
    });
    if (owlResp.ok) {
      const result = await owlResp.json() as any;
      const bindings = result.results?.bindings || [];
      for (const b of bindings) {
        if (b.prop?.value) owl.properties.push(b.prop.value);
        if (b.range?.value) owl.relationships.push(b.range.value);
      }
    }
  } catch { /* fuseki OWL query failed */ }

  // Source 6: Infrastructure — LaunchAgents, endpoints, monitoring
  try {
    // LaunchAgents matching domain
    // Match domain stem without plural (e.g. "seed" not "seeds") to catch com.chorus.seed-probe
    const domainStem = domain.replace(/s$/, '');
    const { stdout: agents } = await execAsync(`launchctl list 2>/dev/null | grep -iE "${domain}|${domainStem}" | grep "com.chorus" || true`, { encoding: 'utf-8', timeout: 5000 });
    infra.launchagents = agents.split('\n').filter((l: string) => l.trim()).map((l: string) => l.trim());

    // Known endpoints for this domain (from domain page or hardcoded registry)
    if (domain === 'seeds') {
      infra.endpoints = ['GET /api/chorus/seeds (3340)', 'POST /webhook/sms (3000)', 'GET /seeds (3000)'];
      infra.monitoring = ['seed-probe LaunchAgent'];
    } else if (domain === 'chorus') {
      infra.endpoints = ['GET /api/chorus/* (3340)', 'Socket.IO (3470)', 'POST /api/nudge (3475)'];
      infra.monitoring = ['heartbeat LaunchAgent', 'alert-notifier LaunchAgent'];
    }
  } catch { /* infra lookup failed */ }

  // Source 7: Jeff's feedback from memory files
  try {
    const memoryDir = path.join(os.homedir(), '.claude/projects/-Users-jeffbridwell-CascadeProjects/memory');
    if (fs.existsSync(memoryDir)) {
      const files = fs.readdirSync(memoryDir).filter(f => f.startsWith('feedback_'));
      for (const file of files) {
        const content = fs.readFileSync(path.join(memoryDir, file), 'utf-8');
        if (content.toLowerCase().includes(domain)) {
          const nameMatch = content.match(/^name:\s*(.+)/m);
          history.feedback.push(nameMatch ? nameMatch[1] : file);
        }
      }
    }
  } catch { /* memory dir not readable */ }

  // Source 8: Code files from instances graph (#1868 — replaces hardcoded domainDirs map)
  const codeScan: { scanned: string[]; discovered: string[] } = { scanned: [], discovered: [] };
  try {
    const domainSuffix = domain.endsWith('-domain') || domain.endsWith('-service') ? domain : `${domain}-domain`;
    const codeQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domainSuffix}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
    const codeResult = await athenaSparqlQuery(codeQuery);
    codeScan.scanned = codeResult.results.bindings.map((b: any) => b.filePath.value);
    // Also try without -domain suffix for service domains
    if (codeScan.scanned.length === 0 && !domain.endsWith('-service')) {
      const svcQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domain}-service> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
      const svcResult = await athenaSparqlQuery(svcQuery);
      codeScan.scanned = svcResult.results.bindings.map((b: any) => b.filePath.value);
    }
  } catch { /* graph query failed — no code data */ }

  // Source 9: Loki logs — recent domain-related entries (#1959)
  const logs: Array<{ timestamp: string; level: string; message: string; component: string }> = [];
  try {
    const domainStem = domain.replace(/s$/, '');
    const lokiQuery = encodeURIComponent(`{job="gathering-app"} |~ "${domain}|${domainStem}" | json`);
    const now = Math.floor(Date.now() / 1000);
    const start = now - 86400; // last 24h
    const lokiResp = await fetch(
      `http://localhost:3102/loki/api/v1/query_range?query=${lokiQuery}&start=${start}&end=${now}&limit=20`
    );
    if (lokiResp.ok) {
      const lokiData = await lokiResp.json() as any;
      for (const stream of lokiData.data?.result || []) {
        for (const [_ts, line] of stream.values || []) {
          try {
            const entry = JSON.parse(line);
            logs.push({
              timestamp: entry.timestamp || '',
              level: entry.level || 'info',
              message: (entry.message || '').slice(0, 200),
              component: entry.component || '',
            });
          } catch { /* skip non-JSON lines */ }
        }
      }
      // Sort: errors first, then by timestamp desc
      logs.sort((a, b) => {
        const levelOrder: Record<string, number> = { error: 0, warn: 1, info: 2 };
        const la = levelOrder[a.level] ?? 3;
        const lb = levelOrder[b.level] ?? 3;
        if (la !== lb) return la - lb;
        return b.timestamp.localeCompare(a.timestamp);
      });
    }
  } catch { /* loki not available */ }

  // Source 10: Alerting rules from alerting/ directory (#1959)
  const alerts: Array<{ name: string; severity: string; file: string }> = [];
  try {
    const alertDir = path.resolve(__dirname, '../../../alerting');
    if (fs.existsSync(alertDir)) {
      const domainStem = domain.replace(/s$/, '');
      const files = fs.readdirSync(alertDir).filter(f => f.endsWith('.yml') || f.endsWith('.yaml'));
      for (const file of files) {
        const content = fs.readFileSync(path.join(alertDir, file), 'utf-8');
        const lower = content.toLowerCase();
        if (lower.includes(domain) || lower.includes(domainStem) || file.toLowerCase().includes(domain) || file.toLowerCase().includes(domainStem)) {
          const nameMatch = content.match(/alert:\s*(.+)/);
          const sevMatch = content.match(/severity:\s*(.+)/);
          alerts.push({
            name: nameMatch ? nameMatch[1].trim() : file.replace(/\.ya?ml$/, ''),
            severity: sevMatch ? sevMatch[1].trim() : 'unknown',
            file,
          });
        }
      }
    }
  } catch { /* alerting dir not readable */ }

  // Calculate trust score: balance unresolved issues against completed work and activity
  const unresolvedCount = history.unresolved.length;
  const feedbackCount = history.feedback.length;
  const doneCount = cards.filter(c => c.status === 'Done').length;
  const recentSpine = spine.filter(s => s.event === 'card.accepted').length;
  const activityBonus = Math.min(40, (doneCount * 10) + (recentSpine * 5));
  history.trust_score = Math.max(0, Math.min(100, 50 + activityBonus - (unresolvedCount * 10) - (feedbackCount * 5)));
  history.health = history.trust_score >= 60 ? 'healthy' : history.trust_score >= 30 ? 'attention' : 'concern';

  // Sort timeline chronologically
  timeline.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));

  res.json({
    domain,
    cards,
    rdf,
    owl,
    mentions,
    spine,
    code,
    codeScan,
    infra,
    logs,
    alerts,
    links,
    related,
    history,
    timeline,
    count: timeline.length,
  });
});

// --- GET /api/chorus/domain/:domain/code-files --- DEPRECATED by #2060
// Replaced by GET /api/chorus/domain/:name/code (consolidated domain API).
// Kept temporarily for backwards compatibility — remove after confirming no consumers.
import { fetchChorusCodeFiles } from './handlers/chorus-code-files';
app.get('/api/chorus/domain/:domain/code-files', async (req: Request, res: Response) => {
  const r = await fetchChorusCodeFiles({ sparql: athenaSparqlQuery }, req.params.domain);
  res.status(r.status).json(r.body);
});

// --- Consolidated domain facet API (#2060) ---
// One endpoint per facet under /api/chorus/domain/:name/.
// AX = UX: same shape whether rendering for Jeff or briefing a role on /pull.

/** Resolve a domain name to its subdomain ID in the ontology.
 *  "seeds" → "seeds-domain", "seeds-domain" → "seeds-domain", "tests-service" → "tests-service" */
async function resolveSubdomainId(name: string): Promise<string> {
  const lower = name.toLowerCase();
  if (lower.endsWith('-domain') || lower.endsWith('-service')) return lower;
  // Try -domain first (most common), then -service
  const domainId = `${lower}-domain`;
  const svcId = `${lower}-service`;
  const checkQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <urn:chorus:ontology> { <https://jeffbridwell.com/chorus#${domainId}> a chorus:SubDomain } }`;
  try {
    const result = await athenaSparqlQuery(checkQuery);
    if (result.boolean) return domainId;
  } catch { /* fall through */ }
  return svcId;
}

const isTestFile = (p: string) => /\/(tests?|__tests__)\//i.test(p) || /\.(test|spec)\./i.test(p) || /\.bats$/i.test(p) || /_test\.rs$/i.test(p) || /\.feature$/i.test(p);

// GET /api/chorus/domain/:name/code — source files for a domain (#2060 AC1)
import { fetchChorusDomainCode } from './handlers/chorus-domain-code';
app.get('/api/chorus/domain/:name/code', async (req: Request, res: Response) => {
  const r = await fetchChorusDomainCode(
    { sparql: athenaSparqlQuery, resolveSubdomainId, envelope: athenaEnvelope },
    req.params.name,
  );
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/tests — test files covering a domain (#2098: unified via quality scanner)
// Previously queried Fuseki for TestCoverage triples. Now proxies to the quality scanner
// so domain-detail and quality-service page show the same test data.
// Extracted to handlers/domain-facets.ts (#2173 AC4).
import {
  fetchDomainTests,
  fetchDomainLogs,
  fetchDomainServices,
  fetchDomainDecisions,
  fetchDomainRadius,
  fetchDomainBlastRadius,
  fetchDomainAlerts,
  fetchDomainInfra,
} from './handlers/domain-facets';

function readAlertFiles(): Array<{ file: string; content: string }> {
  const ALERTS_DIR = path.join(REPO_ROOT, 'proving/domains/alerts');
  return fs.readdirSync(ALERTS_DIR)
    .filter((f: string) => f.endsWith('.yml'))
    .map((f: string) => ({ file: f, content: fs.readFileSync(path.join(ALERTS_DIR, f), 'utf-8') }));
}
const domainFacetDeps = () => ({
  sparql: athenaSparqlQuery,
  resolveSubdomainId,
  envelope: athenaEnvelope,
});

app.get('/api/chorus/domain/:name/tests', async (req: Request, res: Response) => {
  const r = await fetchDomainTests(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/alerts — alert rules for a domain (#2060 AC3)
app.get('/api/chorus/domain/:name/alerts', async (req: Request, res: Response) => {
  const r = await fetchDomainAlerts({ ...domainFacetDeps(), readAlertFiles }, req.params.name);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/logs — log sources for a domain (#2060 AC4)
app.get('/api/chorus/domain/:name/logs', async (req: Request, res: Response) => {
  const r = await fetchDomainLogs(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/services — API endpoints in a domain (#2060 AC5)
app.get('/api/chorus/domain/:name/services', async (req: Request, res: Response) => {
  const r = await fetchDomainServices(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/domain/:name/decisions', async (req: Request, res: Response) => {
  const r = await fetchDomainDecisions(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/domain/:name/radius', async (req: Request, res: Response) => {
  const r = await fetchDomainRadius(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/domain/:name/blast-radius', async (req: Request, res: Response) => {
  const r = await fetchDomainBlastRadius(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/releases — domain-scoped deploy history (#1910)
// Git-first: parse ACP commits, match card domain tags, return newest first.
import { fetchChorusDomainReleases } from './handlers/chorus-domain-releases';
app.get('/api/chorus/domain/:name/releases', async (req: Request, res: Response) => {
  const r = fetchChorusDomainReleases(
    {
      gitLog: () => {
        const { execSync } = require('child_process');
        return execSync('git log --oneline --format="%H|%aI|%s"', {
          cwd: REPO_ROOT, encoding: 'utf-8', timeout: 10000,
        });
      },
      getCards: getBoardCards,
      envelope: athenaEnvelope,
    },
    req.params.name,
  );
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/dependencies — upstream/downstream + shared infra (#2082)
import { fetchChorusDomainDependencies } from './handlers/chorus-domain-dependencies';
app.get('/api/chorus/domain/:name/dependencies', async (req: Request, res: Response) => {
  const r = await fetchChorusDomainDependencies(
    { sparql: athenaSparqlQuery, resolveSubdomainId, envelope: athenaEnvelope },
    req.params.name,
  );
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/infra — borg environments for a domain (#2080)
// Queries urn:borg:instances graph for domain-scoped environments via usesEnvironment edges.
app.get('/api/chorus/domain/:name/infra', async (req: Request, res: Response) => {
  const r = await fetchDomainInfra(domainFacetDeps(), req.params.name);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/domain/:name/pipeline — value stream lifecycle (#2069)
// Assembles from 5 existing sources: cards, completeness, code/tests/endpoints, alerts/gates, done cards.
import { fetchChorusDomainPipeline } from './handlers/chorus-domain-pipeline';
app.get('/api/chorus/domain/:name/pipeline', async (req: Request, res: Response) => {
  const r = await fetchChorusDomainPipeline(
    {
      fetcher: async (relUrl) => {
        try {
          const resp = await fetch(`http://localhost:3340${relUrl}`);
          return resp.ok ? await resp.json() : null;
        } catch { return null; }
      },
      resolveSubdomainId,
      envelope: athenaEnvelope,
    },
    req.params.name,
  );
  res.status(r.status).json(r.body);
});

/** Check if a date falls in US Eastern Daylight Time */
function isEDT(dateStr: string): boolean {
  // Approximate: EDT is second Sunday of March to first Sunday of November
  const d = new Date(dateStr);
  const month = d.getMonth(); // 0-indexed
  if (month > 2 && month < 10) return true; // Apr-Oct always EDT
  if (month === 2) {
    // March: EDT starts second Sunday
    const firstDay = new Date(d.getFullYear(), 2, 1).getDay();
    const secondSunday = firstDay === 0 ? 8 : 15 - firstDay;
    return d.getDate() >= secondSunday;
  }
  if (month === 10) {
    // November: EDT ends first Sunday
    const firstDay = new Date(d.getFullYear(), 10, 1).getDay();
    const firstSunday = firstDay === 0 ? 1 : 8 - firstDay;
    return d.getDate() < firstSunday;
  }
  return false; // Dec-Feb always EST
}

/** Boston timestamp — one conversion point for all display (#1826) */
function bostonNow(): string {
  return convertToLocal(new Date().toISOString(), 'America/New_York');
}

/** Convert ISO timestamp to local time string */
function convertToLocal(isoTimestamp: string, _tz: string): string {
  try {
    const d = new Date(isoTimestamp);
    // Use Intl for proper timezone conversion
    const fmt = new Intl.DateTimeFormat('en-US', {
      timeZone: 'America/New_York',
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit',
      hour12: false,
    });
    const parts = fmt.formatToParts(d);
    const get = (type: string) => parts.find(p => p.type === type)?.value || '';
    return `${get('year')}-${get('month')}-${get('day')} ${get('hour')}:${get('minute')}:${get('second')}`;
  } catch {
    return isoTimestamp;
  }
}

/** Reciprocal Rank Fusion — merge FTS + semantic results by message ID */
// #2168 AC-14: query-aware RRF weighting extracted to ./search-rrf.ts
// so it can be unit-tested without pulling the whole server module.
import { hasExactToken, mergeRRF } from './search-rrf';
export { hasExactToken, mergeRRF };

// --- GET /api/chorus/reconcile ---

import { fetchChorusReconcile } from './handlers/chorus-reconcile';
app.get('/api/chorus/reconcile', (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    addStaleHeader(res, db);
    const r = fetchChorusReconcile({ db }, { role: req.query.role as string | undefined });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/refs ---

import { fetchChorusRefs } from './handlers/chorus-refs';
app.get('/api/chorus/refs', (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    addStaleHeader(res, db);
    const r = fetchChorusRefs({ db }, {
      card: req.query.card as string | undefined,
      wf: req.query.wf as string | undefined,
      type: req.query.type as string | undefined,
      entityId: req.query.id as string | undefined,
    });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/stats ---

import { fetchChorusStats } from './handlers/chorus-stats';
app.get('/api/chorus/stats', (_req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    addStaleHeader(res, db);
    const r = fetchChorusStats({ db });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/freshness (#1879) ---
// Per-source freshness with graduated staleness levels

const SOURCE_CADENCE: Record<string, number> = {
  claude: 3600,        // 1h — session data should be near-realtime
  spine: 3600,         // 1h
  brief: 86400,        // 24h
  decision: 86400,     // 24h
  clearing: 86400,     // 24h
  memory: 86400,       // 24h
  story: 86400,        // 24h
  adr: 604800,         // 7d
  activity: 86400,     // 24h
  state: 86400,        // 24h
  crawler: 86400,      // 24h
  journal: 604800,     // 7d
};

import { fetchFreshness } from './handlers/chorus-freshness';
app.get('/api/chorus/freshness', (_req: Request, res: Response) => {
  if (!fs.existsSync(DB_PATH)) {
    res.status(503).json({ error: 'Index database not found' });
    return;
  }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    const r = fetchFreshness({
      db,
      exists: (p) => fs.existsSync(p),
      readFile: (p, enc) => fs.readFileSync(p, enc),
      spineLogPath: path.join(REPO_ROOT, 'platform/logs/chorus.log'),
      cadence: SOURCE_CADENCE,
      timestamp: bostonNow,
    });
    res.status(r.status).json(r.body);
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/pulse/latest (#1881) ---
// Returns most recent Pulse team state snapshot

import { fetchChorusPulseLatest } from './handlers/chorus-pulse-latest';
app.get('/api/chorus/pulse/latest', (_req: Request, res: Response) => {
  const r = fetchChorusPulseLatest({
    readPulse: () => fs.existsSync('/tmp/pulse-latest.json')
      ? fs.readFileSync('/tmp/pulse-latest.json', 'utf-8')
      : null,
  });
  res.status(r.status).json(r.body);
});

// --- POST /api/chorus/reindex (#1879) ---
// Trigger full re-index + re-embed without app restart

app.post('/api/chorus/reindex', async (_req: Request, res: Response) => {
  try {
    const indexResult = await indexAllSources();
    const embedResult = await embedDelta();
    res.json({
      status: 'ok',
      ...indexResult,
      embedded: embedResult.embedded,
      skipped: embedResult.skipped,
      timestamp: bostonNow(),
    });
  } catch (err: any) {
    res.status(500).json({ error: err.message });
  }
});

// --- POST /api/chorus/index ---
// Inline indexing — replaces deleted bash scripts (#1879)

app.post('/api/chorus/index', async (_req: Request, res: Response) => {
  try {
    const result = await indexAllSources();
    embedDelta().catch(err =>
      console.error(`[embed-delta] post-index embed failed: ${err.message}`)
    );
    res.json(result);
  } catch (err: any) {
    res.status(500).json({ error: err.message });
  }
});

/** Index all file-based sources into the Chorus SQLite DB (#1879) */
async function indexAllSources(): Promise<Record<string, any>> {
  const db = new Database(DB_PATH);
  db.pragma('journal_mode = WAL');
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

  const now = new Date().toISOString();

  // 1. Spine events from chorus.log
  try {
    const logPath = path.join(REPO_ROOT, 'platform/logs/chorus.log');
    if (fs.existsSync(logPath)) {
      const content = fs.readFileSync(logPath, 'utf-8');
      const lines = content.trim().split('\n');
      let indexed = 0;
      const insertMany = db.transaction((events: any[]) => {
        for (const e of events) {
          insert.run(e.source, e.source_id, e.channel, e.role, e.author, e.content, e.timestamp);
          indexed++;
        }
      });
      const events: any[] = [];
      for (const line of lines) { // last 5000 lines
        try {
          const evt = JSON.parse(line);
          const role = evt.role || 'system';
          const event = evt.event || 'unknown';
          events.push({
            source: 'spine',
            source_id: `spine-${evt.timestamp}-${event}`,
            channel: `spine:${role}`,
            role,
            author: role,
            content: line,
            timestamp: evt.timestamp || now,
          });
        } catch { /* skip malformed */ }
      }
      insertMany(events);
      updateWatermark.run('spine', now, now);
      results.spine = `${indexed} events indexed`;
    }
  } catch (err: any) { results.spine = `error: ${err.message}`; }

  // 2. Briefs
  try {
    let indexed = 0;
    for (const role of ['wren', 'silas', 'kade']) {
      const briefDir = path.join(REPO_ROOT, `roles/${role}/briefs`);
      if (!fs.existsSync(briefDir)) continue;
      const files = fs.readdirSync(briefDir).filter(f => f.endsWith('.md') && !f.startsWith('.'));
      for (const file of files) {
        const filePath = path.join(briefDir, file);
        const stat = fs.statSync(filePath);
        const content = fs.readFileSync(filePath, 'utf-8');
        const ts = stat.mtime.toISOString();
        insert.run('brief', `brief:${role}:${file}`, `brief:${role}`, role, role, content, ts);
        indexed++;
      }
    }
    updateWatermark.run('artifact:brief', now, now);
    results.briefs = `${indexed} briefs indexed`;
  } catch (err: any) { results.briefs = `error: ${err.message}`; }

  // 3. Decisions
  try {
    const decPath = path.join(REPO_ROOT, 'roles/wren/decisions.md');
    if (fs.existsSync(decPath)) {
      const content = fs.readFileSync(decPath, 'utf-8');
      const decisions = content.split('\n## DEC-').filter(Boolean);
      let indexed = 0;
      for (const dec of decisions) {
        const firstLine = dec.split('\n')[0];
        const id = firstLine.match(/^(\d+)/)?.[1] || 'unknown';
        insert.run('decision', `decision:DEC-${id}`, 'decisions', 'wren', 'wren', `## DEC-${dec}`, now);
        indexed++;
      }
      updateWatermark.run('artifact:decisions', now, now);
      results.decisions = `${indexed} decisions indexed`;
    }
  } catch (err: any) { results.decisions = `error: ${err.message}`; }

  // 4. ADRs
  try {
    const adrDir = path.join(REPO_ROOT, 'roles/silas/adr');
    if (fs.existsSync(adrDir)) {
      const files = fs.readdirSync(adrDir).filter(f => f.endsWith('.md'));
      let indexed = 0;
      for (const file of files) {
        const content = fs.readFileSync(path.join(adrDir, file), 'utf-8');
        insert.run('adr', `adr:${file}`, 'adr:silas', 'silas', 'silas', content, now);
        indexed++;
      }
      updateWatermark.run('artifact:adr', now, now);
      results.adrs = `${indexed} ADRs indexed`;
    }
  } catch (err: any) { results.adrs = `error: ${err.message}`; }

  // 5. Activity log
  try {
    const actPath = path.join(REPO_ROOT, 'activity.md');
    if (fs.existsSync(actPath)) {
      const content = fs.readFileSync(actPath, 'utf-8');
      insert.run('activity', 'activity:latest', 'activity', 'system', 'system', content, now);
      updateWatermark.run('artifact:activity', now, now);
      results.activity = 'indexed';
    }
  } catch (err: any) { results.activity = `error: ${err.message}`; }

  // 6. Memory files
  try {
    const memDir = path.join(os.homedir(), '.claude/projects');
    if (fs.existsSync(memDir)) {
      let indexed = 0;
      // Find memory directories
      const dirs = fs.readdirSync(memDir).filter(d => d.includes('chorus'));
      for (const dir of dirs) {
        const memoryDir = path.join(memDir, dir, 'memory');
        if (!fs.existsSync(memoryDir)) continue;
        const files = fs.readdirSync(memoryDir).filter(f => f.endsWith('.md'));
        for (const file of files) {
          const content = fs.readFileSync(path.join(memoryDir, file), 'utf-8');
          insert.run('memory', `memory:${file}`, 'memory', 'system', 'system', content, now);
          indexed++;
        }
      }
      updateWatermark.run('artifact:memory', now, now);
      results.memory = `${indexed} memory files indexed`;
    }
  } catch (err: any) { results.memory = `error: ${err.message}`; }

  // 7. State files (next-session.md per role)
  try {
    let indexed = 0;
    for (const role of ['wren', 'silas', 'kade']) {
      const nsPath = path.join(REPO_ROOT, `roles/${role}/next-session.md`);
      if (fs.existsSync(nsPath)) {
        const content = fs.readFileSync(nsPath, 'utf-8');
        insert.run('state', `state:${role}:next-session`, `state:${role}`, role, role, content, now);
        indexed++;
      }
    }
    updateWatermark.run('artifact:state', now, now);
    results.state = `${indexed} state files indexed`;
  } catch (err: any) { results.state = `error: ${err.message}`; }

  // 8. Clearing/chat transcripts
  try {
    const chatDir = '/tmp/chorus-chat';
    if (fs.existsSync(chatDir)) {
      let indexed = 0;
      const files = fs.readdirSync(chatDir).filter(f => f.endsWith('.md'));
      for (const file of files) {
        const content = fs.readFileSync(path.join(chatDir, file), 'utf-8');
        const stat = fs.statSync(path.join(chatDir, file));
        insert.run('clearing', `clearing:${file}`, 'clearing:session', 'system', 'system', content, stat.mtime.toISOString());
        indexed++;
      }
      updateWatermark.run('clearing', now, now);
      results.clearing = `${indexed} transcripts indexed`;
    }
  } catch (err: any) { results.clearing = `error: ${err.message}`; }

  // 9. Journal entries per role
  try {
    let indexed = 0;
    for (const role of ['wren', 'silas', 'kade']) {
      const journalDir = path.join(REPO_ROOT, `roles/${role}/journal`);
      if (!fs.existsSync(journalDir)) continue;
      const files = fs.readdirSync(journalDir).filter(f => f.endsWith('.md'));
      for (const file of files) {
        const content = fs.readFileSync(path.join(journalDir, file), 'utf-8');
        insert.run('journal', `journal:${role}:${file}`, `journal:${role}`, role, role, content, now);
        indexed++;
      }
    }
    updateWatermark.run('journal', now, now);
    results.journal = `${indexed} journal entries indexed`;
  } catch (err: any) { results.journal = `error: ${err.message}`; }

  // 10. Stories
  try {
    let indexed = 0;
    const storiesFile = path.join(REPO_ROOT, 'roles/wren/self-stories.md');
    if (fs.existsSync(storiesFile)) {
      const content = fs.readFileSync(storiesFile, 'utf-8');
      const stories = content.split('\n## ').filter(Boolean);
      for (const story of stories) {
        const title = story.split('\n')[0].trim();
        const id = title.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase().slice(0, 50);
        insert.run('story', `story:${id}`, 'stories', 'wren', 'jeff', `## ${story}`, now);
        indexed++;
      }
    }
    // Story briefs in archive
    const archiveDir = path.join(REPO_ROOT, 'roles/wren/briefs-archive');
    if (fs.existsSync(archiveDir)) {
      const storyFiles = fs.readdirSync(archiveDir).filter(f => f.includes('story'));
      for (const file of storyFiles) {
        const content = fs.readFileSync(path.join(archiveDir, file), 'utf-8');
        insert.run('story', `story:brief:${file}`, 'stories', 'wren', 'jeff', content, now);
        indexed++;
      }
    }
    updateWatermark.run('stories', now, now);
    results.stories = `${indexed} stories indexed`;
  } catch (err: any) { results.stories = `error: ${err.message}`; }

  // 11. Slack — deprecated, remove watermark and old messages
  try {
    db.prepare(`DELETE FROM watermarks WHERE source LIKE 'slack%'`).run();
    db.prepare(`DELETE FROM watermarks WHERE source = 'slack'`).run();
    results.slack = 'removed (deprecated)';
  } catch { /* ignore */ }

  db.close();

  return {
    indexed: results,
    elapsed_ms: Date.now() - startTime,
  };
}

// --- GET /api/chorus/self — Read-only filtered endpoint for Self (DEC-068) ---
// Source whitelist: memory, story, decision, brief, adr
// Blocks: claude (raw sessions), spine (ops events), slack, clearing, activity, state

const SELF_SOURCE_WHITELIST = new Set(['memory', 'story', 'decision', 'brief', 'adr']);

import { fetchSelf } from './handlers/chorus-self';
app.get('/api/chorus/self', async (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    const r = await fetchSelf(
      {
        db,
        semanticSearch: lanceTable ? (semanticSearch as unknown as import('./handlers/chorus-self').SemanticSearchFn) : undefined,
        sparqlSearch: sparqlSearch as unknown as import('./handlers/chorus-self').SparqlSearchFn,
        mergeUnified: mergeUnified as unknown as import('./handlers/chorus-self').MergeUnifiedFn,
        emitSearchEvent,
        whitelist: SELF_SOURCE_WHITELIST,
      },
      { q: req.query.q as string | undefined, limit: req.query.limit as string | undefined },
    );
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- POST /api/chorus/embed (trigger embed-delta on demand) ---

app.post('/api/chorus/embed', async (_req: Request, res: Response) => {
  try {
    const result = await embedDelta();
    res.json(result);
  } catch (err: any) {
    res.status(500).json({ error: err.message });
  }
});

// --- POST /api/chorus/pulse (spine event emission — replaces chorus-log.sh) ---

app.post('/api/chorus/pulse', (req: Request, res: Response) => {
  const CHORUS_LOG = `${CHORUS_ROOT}/platform/logs/chorus.log`;
  const { event, role, level, ...extras } = req.body || {};

  if (!event || !role) {
    res.status(400).json({ error: 'event and role are required' });
    return;
  }

  const validLevels = ['info', 'warn', 'critical'];
  const safeLevel = validLevels.includes(level) ? level : 'info';

  const entry: Record<string, string> = {
    timestamp: bostonNow(),
    level: safeLevel,
    appName: 'chorus-events',
    component: 'lifecycle',
    event,
    role,
  };

  // Merge extra key-value pairs (card, domain, etc.)
  for (const [k, v] of Object.entries(extras)) {
    if (typeof v === 'string' || typeof v === 'number') {
      entry[k] = String(v);
    }
  }

  const line = JSON.stringify(entry);
  fs.appendFileSync(CHORUS_LOG, line + '\n');

  res.json({ ok: true, event, role, level: safeLevel });
});

// --- POST /api/chorus/role-state (replaces role-state.sh) ---

app.post('/api/chorus/role-state', (req: Request, res: Response) => {
  const { role, state, card, type: cardType } = req.body || {};

  if (!role || !state) {
    res.status(400).json({ error: 'role and state are required' });
    return;
  }

  const validStates = ['building', 'blocked', 'waiting', 'observing', 'idle'];
  if (!validStates.includes(state)) {
    res.status(400).json({ error: `Invalid state '${state}'. Use: ${validStates.join(', ')}` });
    return;
  }

  const stateFile = `/tmp/role-state-${role}.json`;
  const ts = new Date().toISOString();
  const stateData = { role, state, card: card || null, type: cardType || null, updated: ts };

  fs.writeFileSync(stateFile, JSON.stringify(stateData, null, 2));

  // Also emit as spine event
  const CHORUS_LOG = `${CHORUS_ROOT}/platform/logs/chorus.log`;
  const entry = JSON.stringify({
    timestamp: ts,
    level: 'info',
    appName: 'chorus-events',
    component: 'lifecycle',
    event: 'role.state.changed',
    role,
    state,
    ...(card ? { card: String(card) } : {}),
    ...(cardType ? { type: cardType } : {}),
  });
  fs.appendFileSync(CHORUS_LOG, entry + '\n');

  res.json({ ok: true, role, state, card: card || null });
});

// --- POST /api/chorus/alert (Grafana webhook receiver) ---

app.post('/api/chorus/alert', (req: Request, res: Response) => {
  const CHORUS_LOG = `${CHORUS_ROOT}/platform/logs/chorus.log`;
  const alerts = req.body?.alerts || [];
  const ts = new Date().toISOString();

  for (const alert of alerts) {
    const severity = alert.labels?.severity || 'unknown';
    const alertname = alert.labels?.alertname || 'unknown';
    const status = alert.status || 'unknown'; // firing or resolved
    const summary = alert.annotations?.summary || '';
    const description = alert.annotations?.description || '';

    const entry = JSON.stringify({
      timestamp: ts,
      level: severity === 'critical' ? 'error' : 'warn',
      appName: 'grafana-alerts',
      component: 'alertmanager',
      event: `alert_${status}`,
      role: 'system',
      alertname,
      severity,
      summary,
      description: description.substring(0, 500),
    });

    fs.appendFileSync(CHORUS_LOG, entry + '\n');

    // macOS desktop notification for critical/firing alerts
    if (severity === 'critical' && status === 'firing') {
      const notifTitle = `ALERT: ${alertname}`;
      const notifMsg = summary || description.substring(0, 100);
      execFile('osascript', ['-e',
        `display notification "${notifMsg.replace(/"/g, '\\"')}" with title "${notifTitle.replace(/"/g, '\\"')}" sound name "Basso"`
      ], (err) => { if (err) console.error('Notification failed:', err.message); });
    }
  }

  res.json({ received: alerts.length });
});

// --- GET /api/chorus/voice-analytics ---

app.get('/api/chorus/voice-analytics', (req: Request, res: Response) => {
  const days = Math.min(Math.max(parseInt(req.query.days as string || '30', 10), 1), 365);
  const cutoff = new Date(Date.now() - days * 24 * 60 * 60 * 1000).toISOString();

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    addStaleHeader(res, db);

    const rows = db.prepare(`
      SELECT content, channel, timestamp FROM messages
      WHERE author='user' AND source='claude'
        AND channel IN ('session:wren','session:silas','session:kade')
        AND timestamp >= ?
      ORDER BY timestamp ASC
    `).all(cutoff) as { content: string; channel: string; timestamp: string }[];

    // --- Content filter: skip system context, tool outputs, skill injections, very long messages ---
    const filtered = rows.map(r => {
      // Strip system-reminder blocks that get appended to user messages
      const cleaned = r.content.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, '').trim();
      return { ...r, content: cleaned };
    }).filter(r => {
      const c = r.content;
      if (!c || c.length > 500) return false; // system context is typically long
      if (c.startsWith('<') || c.startsWith('{') || c.startsWith('[')) return false; // XML/JSON
      if (/^\/Users\/|^\/tmp\/|^\/opt\//.test(c)) return false; // file paths
      if (/^(exit|y|n|yes|no)$/i.test(c.trim())) return false; // single-word responses
      // Skill prompt injections (#1752) — recorded as user messages but are system content
      if (c.includes('Base directory for this skill:')) return false;
      if (c.startsWith('# /') && c.includes('## ')) return false; // skill markdown docs
      if (/^<command-/.test(c)) return false; // command tags
      if (/^<task-notification>/.test(c)) return false; // background task output
      if (c.startsWith('This session is being continued from')) return false; // context summaries
      if (/^ARGUMENTS:/.test(c)) return false; // skill argument blocks
      return true;
    });

    // --- Tone classifier ---
    const CORRECTIVE_PATTERNS = /\bdon'?t\b|\bstop\s|\bno\s|\bnever\b|\bshouldn'?t\b|\bwrong\b|\bthat'?s not\b/i;
    const COLLABORATIVE_PATTERNS = /\blet'?s\b|\bwe could\b|\bwhat if\b|\bwhat do you think\b|\bagree\b/i;
    const IMPERATIVE_VERBS = new Set([
      'add','build','check','clean','close','commit','configure','create','debug','delete',
      'deploy','do','edit','enable','ensure','execute','export','extract','fetch','fix',
      'generate','get','grep','implement','import','install','kill','list','load','look',
      'make','merge','move','open','pipe','pull','push','read','refactor','remove',
      'rename','replace','restart','restore','review','revert','run','save','scan','search',
      'send','set','setup','ship','show','skip','sort','start','stop','strip','switch',
      'tag','test','try','update','upgrade','use','verify','view','wire','write'
    ]);

    // Sub-classify the old 'neutral' catchall (#1752)
    const ACKNOWLEDGMENT_PATTERNS = /^(ok|okay|cool|yep|yup|yeah|sure|right|sounds good|makes sense|got it|ack|perfect|great|nice|good|fair|fine|agreed|copy|roger|indeed|exactly|absolutely|certainly|totally|100%)\b/i;
    const ROUTING_PATTERNS = /\btake a look\b|\bcheck out\b|\bgo to\b|\bchat w|\btalk to\b|\bloop .* in\b|^\/\w+\s|@\w+|\btail\s|\.sh\b|\.html\b|\.md\b|localhost/i;
    const STATUS_PATTERNS = /\bi (just|already|was|am|have been|went|did|tried|restarted|stopped|started|rebooted|logged|walked|finished|completed|shipped)\b/i;

    function classifyTone(text: string): string {
      if (CORRECTIVE_PATTERNS.test(text)) return 'corrective';
      if (text.includes('?')) return 'question';
      const firstWord = text.trim().split(/\s+/)[0]?.toLowerCase().replace(/[^a-z]/g, '');
      if (firstWord && IMPERATIVE_VERBS.has(firstWord)) return 'directive';
      if (COLLABORATIVE_PATTERNS.test(text)) return 'collaborative';
      // Sub-classify neutral (#1752)
      const trimmed = text.trim();
      if (ACKNOWLEDGMENT_PATTERNS.test(trimmed)) return 'acknowledgment';
      if (ROUTING_PATTERNS.test(trimmed)) return 'routing';
      if (STATUS_PATTERNS.test(trimmed)) return 'status';
      return 'narrative';
    }

    // --- Single-pass aggregation ---
    const TONE_KEYS = ['directive', 'collaborative', 'question', 'corrective', 'acknowledgment', 'routing', 'status', 'narrative'];
    const initTones = () => Object.fromEntries(TONE_KEYS.map(k => [k, 0]));
    const toneCount: Record<string, number> = initTones();
    const toneByRole: Record<string, Record<string, number>> = {
      wren: initTones(),
      silas: initTones(),
      kade: initTones(),
    };
    // Weekly buckets keyed by ISO week
    const weeklyTone: Record<string, Record<string, number>> = {};
    const weeklyRole: Record<string, Record<string, number>> = {};
    const weeklyLength: Record<string, { total: number; count: number }> = {};
    // Hour of day (Boston — DST-aware)
    const hourByRole: Record<string, number[]> = {
      wren: new Array(24).fill(0),
      silas: new Array(24).fill(0),
      kade: new Array(24).fill(0),
    };
    // Bigrams
    const bigramCount: Record<string, number> = {};
    // Corrective word frequency
    const correctiveWords: Record<string, number> = {};
    // Per-role word frequency for distinctive vocab
    const roleWordFreq: Record<string, Record<string, number>> = { wren: {}, silas: {}, kade: {} };

    const STOP_WORDS = new Set([
      'the','a','an','and','or','but','in','on','at','to','for','of','is','it','i','you',
      'that','this','with','be','are','was','were','been','have','has','had','do','does',
      'did','will','would','could','should','can','may','might','shall','not','no','so',
      'if','then','than','just','also','very','too','up','out','about','into','over',
      'from','by','as','my','me','we','our','your','its','they','them','their','he','she',
      'his','her','him','what','which','who','how','when','where','why','all','each',
      'some','any','more','most','other','there','here','now','get','got','go','going',
      'like','thats','dont','im','ive','youre','hes','shes','were','theyre','wont',
      'cant','didnt','doesnt','isnt','arent','wasnt','havent','hasnt','lets','need',
      'know','think','want','see','look','make','good','new','one','two','well','way',
      'back','still','thing','things','right','take','come','been','being','much','said',
      // Path fragments and system noise
      'users','jeffbridwell','cascadeprojects','messages','scripts','workflow','bash',
      'tmp','opt','homebrew','usr','bin','local','node','npm','src','dist','var',
      'http','https','localhost','com','json','html','css','tsx','ts','js','md',
      'git','api','app','log','err','true','false','null','undefined',
    ]);

    // Skip numeric-only tokens
    function isValidWord(w: string): boolean {
      return w.length > 2 && !STOP_WORDS.has(w) && !/^\d+$/.test(w);
    }

    const seenContent = new Set<string>(); // dedup for bigrams (#1752)
    for (const row of filtered) {
      const role = row.channel.replace('session:', '') as 'wren' | 'silas' | 'kade';
      const tone = classifyTone(row.content);
      const words = row.content.toLowerCase().replace(/[^a-z0-9\s'-]/g, ' ').split(/\s+/).filter(w => w.length > 1);

      // Tone counts
      toneCount[tone]++;
      if (toneByRole[role]) toneByRole[role][tone]++;

      // Weekly buckets
      const d = new Date(row.timestamp);
      const dayOfYear = Math.floor((d.getTime() - new Date(d.getFullYear(), 0, 1).getTime()) / 86400000);
      const weekNum = Math.ceil((dayOfYear + new Date(d.getFullYear(), 0, 1).getDay() + 1) / 7);
      const weekKey = `${d.getFullYear()}-W${String(weekNum).padStart(2, '0')}`;

      if (!weeklyTone[weekKey]) weeklyTone[weekKey] = Object.fromEntries(TONE_KEYS.map(k => [k, 0]));
      weeklyTone[weekKey][tone]++;

      if (!weeklyRole[weekKey]) weeklyRole[weekKey] = { wren: 0, silas: 0, kade: 0 };
      weeklyRole[weekKey][role]++;

      if (!weeklyLength[weekKey]) weeklyLength[weekKey] = { total: 0, count: 0 };
      weeklyLength[weekKey].total += words.length;
      weeklyLength[weekKey].count++;

      // Hour of day (Boston — DST-aware via isEDT)
      const utcHour = d.getUTCHours();
      const offset = isEDT(d.toISOString().slice(0, 10)) ? 4 : 5;
      const bostonHour = (utcHour - offset + 24) % 24;
      hourByRole[role][bostonHour]++;

      // Bigrams — deduplicate repeated messages (#1752)
      if (!seenContent.has(row.content)) {
        seenContent.add(row.content);
        for (let i = 0; i < words.length - 1; i++) {
          if (isValidWord(words[i]) && isValidWord(words[i + 1])) {
            const bg = `${words[i]} ${words[i + 1]}`;
            bigramCount[bg] = (bigramCount[bg] || 0) + 1;
          }
        }
      }

      // Corrective words
      if (tone === 'corrective') {
        for (const w of words) {
          if (isValidWord(w)) {
            correctiveWords[w] = (correctiveWords[w] || 0) + 1;
          }
        }
      }

      // Per-role word frequency
      for (const w of words) {
        if (isValidWord(w)) {
          roleWordFreq[role][w] = (roleWordFreq[role][w] || 0) + 1;
        }
      }
    }

    // --- Compute results ---
    const total = filtered.length;

    // 1. Headline percentages — all categories including sub-classified neutral (#1752)
    const headline: Record<string, number> = {};
    for (const t of TONE_KEYS) {
      headline[t] = total > 0 ? Math.round((toneCount[t] / total) * 100) : 0;
    }

    // 2. Tone by role (percentages)
    const toneByRolePct: Record<string, Record<string, number>> = {};
    for (const [r, counts] of Object.entries(toneByRole)) {
      const roleTotal = Object.values(counts).reduce((a, b) => a + b, 0);
      toneByRolePct[r] = {};
      for (const [t, c] of Object.entries(counts)) {
        toneByRolePct[r][t] = roleTotal > 0 ? Math.round((c / roleTotal) * 100) : 0;
      }
    }

    // 3. Tone trend (weekly arrays, sorted)
    const weeks = Object.keys(weeklyTone).sort();
    const toneTrend: Record<string, any> = { weeks };
    for (const t of TONE_KEYS) {
      toneTrend[t] = weeks.map(w => weeklyTone[w]?.[t] || 0);
    }

    // 4. Role attention (weekly)
    const roleAttention = {
      weeks,
      wren: weeks.map(w => weeklyRole[w]?.wren || 0),
      silas: weeks.map(w => weeklyRole[w]?.silas || 0),
      kade: weeks.map(w => weeklyRole[w]?.kade || 0),
    };

    // 5. Message length trend
    const messageLengthTrend = {
      weeks,
      avgWords: weeks.map(w => {
        const wl = weeklyLength[w];
        return wl && wl.count > 0 ? Math.round(wl.total / wl.count) : 0;
      }),
    };

    // 6. Hour of day
    const hourOfDay = hourByRole;

    // 7. Top 25 bigrams
    const bigrams = Object.entries(bigramCount)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 25)
      .map(([phrase, count]) => ({ phrase, count }));

    // 8. Top 15 corrective words
    const correctiveWordList = Object.entries(correctiveWords)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 15)
      .map(([word, count]) => ({ word, count }));

    // 9. Distinctive vocabulary per role
    // Words overrepresented vs overall frequency
    const totalWordFreq: Record<string, number> = {};
    for (const freq of Object.values(roleWordFreq)) {
      for (const [w, c] of Object.entries(freq)) {
        totalWordFreq[w] = (totalWordFreq[w] || 0) + c;
      }
    }
    const distinctiveVocab: Record<string, { word: string; count: number }[]> = {};
    for (const [role, freq] of Object.entries(roleWordFreq)) {
      const roleTotal = Object.values(freq).reduce((a, b) => a + b, 0);
      const globalTotal = Object.values(totalWordFreq).reduce((a, b) => a + b, 0);
      const scored = Object.entries(freq)
        .filter(([w, c]) => c >= 5 && totalWordFreq[w] >= 5)
        .map(([w, c]) => {
          const roleRate = c / roleTotal;
          const globalRate = totalWordFreq[w] / globalTotal;
          return { word: w, count: c, ratio: roleRate / globalRate };
        })
        .sort((a, b) => b.ratio - a.ratio)
        .slice(0, 10)
        .map(({ word, count }) => ({ word, count }));
      distinctiveVocab[role] = scored;
    }

    // Date range
    const dateRange = filtered.length > 0
      ? { from: filtered[0].timestamp.split('T')[0], to: filtered[filtered.length - 1].timestamp.split('T')[0] }
      : { from: null, to: null };

    res.json({
      meta: { messages: total, days, dateRange },
      headline,
      toneByRole: toneByRolePct,
      toneTrend,
      roleAttention,
      messageLengthTrend,
      hourOfDay,
      bigrams,
      correctiveWords: correctiveWordList,
      distinctiveVocab,
    });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/reprompt-analytics ---

import { fetchChorusRepromptAnalytics } from './handlers/chorus-reprompt-analytics';
app.get('/api/chorus/reprompt-analytics', (req: Request, res: Response) => {
  let db: Database.Database;
  try { db = getDb(); } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }
  try {
    addStaleHeader(res, db);
    const r = fetchChorusRepromptAnalytics({ db }, { days: req.query.days as string | undefined });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- GET /api/chorus/attention-analytics (extracted #2189) ---
import { fetchAttentionAnalytics } from './handlers/chorus-attention-analytics';
app.get('/api/chorus/attention-analytics', (_req: Request, res: Response) => {
  const r = fetchAttentionAnalytics({
    isEDT,
    tsvPath: '/tmp/claude-team-scan/jeff-intensity-history.tsv',
    statePath: '/tmp/claude-team-scan/jeff-state.json',
    promptDir: '/tmp/claude-team-scan',
  });
  res.status(r.status).json(r.body);
});

// --- POST /api/chorus/voice — Receive audio, transcribe, inject into role session (#1099) ---

const VOICE_DIR = '/tmp/chorus-listen';

app.post('/api/chorus/voice/:role', express.raw({ type: 'audio/*', limit: '10mb' }), (req: Request, res: Response) => {
  const role = req.params.role;
  if (!['wren', 'silas', 'kade'].includes(role)) {
    res.status(400).json({ error: 'Invalid role. Must be wren, silas, or kade.' });
    return;
  }

  if (!req.body || req.body.length === 0) {
    res.status(400).json({ error: 'No audio data received' });
    return;
  }

  // Save audio to temp file
  fs.mkdirSync(VOICE_DIR, { recursive: true });
  const audioPath = path.join(VOICE_DIR, `voice-${role}-${Date.now()}.wav`);
  fs.writeFileSync(audioPath, req.body);

  // Run voice-to-session.sh asynchronously
  const scriptPath = path.join(SCRIPTS_DIR, 'voice-to-session.sh');
  const start = Date.now();

  execFile('bash', [scriptPath, role, audioPath], { timeout: 30000 }, (err, stdout, stderr) => {
    const elapsed = Date.now() - start;
    // Clean up audio file
    try { fs.unlinkSync(audioPath); } catch { /* ignore */ }

    if (err) {
      console.error(`[chorus-api] voice-to-session error: ${err.message}`);
      res.status(500).json({ error: 'Transcription or injection failed', detail: stderr.trim() });
      return;
    }

    const transcript = stderr.replace(/^.*Transcript \(\d+ms\): /, '').trim();
    res.json({
      status: 'injected',
      role,
      transcript,
      latency_ms: elapsed,
    });
  });
});

// --- GET /api/chorus/perf — Latest perf baseline results (#1485) ---

const PERF_SCRIPT = path.join(os.homedir(), 'CascadeProjects/jeff-bridwell-personal-site/scripts/perf-baseline.sh');

import { fetchPerf } from './handlers/chorus-perf';
app.get('/api/chorus/perf', async (_req: Request, res: Response) => {
  const r = await fetchPerf({ scriptPath: PERF_SCRIPT });
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/services — LaunchAgent service status (#1485) ---

import { fetchServices } from './handlers/chorus-services';
app.get('/api/chorus/services', async (_req: Request, res: Response) => {
  const r = await fetchServices();
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/disk — Disk usage summary (#1485, extracted #2189) ---
import { fetchDisk } from './handlers/chorus-disk';
app.get('/api/chorus/disk', async (_req: Request, res: Response) => {
  const r = await fetchDisk();
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/harvest — Harvest pipeline status (#1485, extracted #2189) ---
import { fetchHarvest } from './handlers/chorus-harvest';
app.get('/api/chorus/harvest', async (_req: Request, res: Response) => {
  const r = await fetchHarvest({ fusekiUrl: FUSEKI_URL });
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/cost — Cost summary (#1485) ---

const COST_SCRIPT = path.join(process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus'), 'platform/scripts/cost-report.sh');

import { fetchCost } from './handlers/chorus-cost';
app.get('/api/chorus/cost', async (req: Request, res: Response) => {
  const period = (req.query.period as string) || 'summary';
  const r = await fetchCost(period, { scriptPath: COST_SCRIPT });
  res.status(r.status).json(r.body);
});

// --- Seeds endpoint (#1869, extracted #2189) ---
import { fetchSeeds } from './handlers/chorus-seeds';
app.get('/api/chorus/seeds', async (_req: Request, res: Response) => {
  const r = await fetchSeeds();
  res.status(r.status).json(r.body);
});

// --- Seed media serving (#2007, extracted #2189) ---

const SEED_MEDIA_DIR = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site/data/pods/jeff/capture/media');

import { resolveSeedMedia } from './handlers/chorus-seed-media';
app.get('/api/chorus/seed-media/:filename', (req: Request, res: Response) => {
  const r = resolveSeedMedia(req.params.filename, {
    baseDir: SEED_MEDIA_DIR,
    exists: (p) => fs.existsSync(p),
  });
  if (r.status === 200) { res.sendFile(r.filePath); return; }
  res.status(r.status).json(r.body);
});

// --- Health check ---

app.get('/health', (_req: Request, res: Response) => {
  // Liveness only — no queries, no counts (#1978)
  res.json({ status: 'ok' });
});

// --- ICD Write API (#1549) — mirrors app write endpoints, no auth ---

const FUSEKI_UPDATE_URL = process.env.FUSEKI_UPDATE_URL || 'http://localhost:3030/pods/update';
const FUSEKI_QUERY_URL = process.env.FUSEKI_QUERY_URL || 'http://localhost:3030/pods/sparql';
const ICD_GRAPH = 'https://jeffbridwell.com/icd/current';
const ICD_PFX = 'PREFIX icd: <https://jeffbridwell.com/icd#>';

function escSparql(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '');
}

function icdSlug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

async function icdSparqlUpdate(update: string): Promise<void> {
  const resp = await fetch(FUSEKI_UPDATE_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/sparql-update' },
    body: update,
  });
  if (!resp.ok) {
    const body = await resp.text();
    throw new Error(`SPARQL update failed: ${resp.status} — ${body}`);
  }
}

async function icdSparqlQuery(query: string): Promise<any> {
  const resp = await fetch(`${FUSEKI_QUERY_URL}?query=${encodeURIComponent(query)}`, {
    headers: { Accept: 'application/sparql-results+json' },
  });
  if (!resp.ok) throw new Error(`SPARQL query failed: ${resp.status}`);
  return resp.json();
}

async function resolveIcdDomain(domainId: string): Promise<string | null> {
  const r = await icdSparqlQuery(`${ICD_PFX} SELECT ?d WHERE { GRAPH <${ICD_GRAPH}> { ?d a icd:Domain ; icd:domainId ?did . FILTER(?did = "domain-${domainId}") } } LIMIT 1`);
  if (r.results.bindings.length > 0) return r.results.bindings[0].d.value;
  const r2 = await icdSparqlQuery(`${ICD_PFX} SELECT ?d WHERE { GRAPH <${ICD_GRAPH}> { ?d a icd:Domain ; icd:domainName ?name . FILTER(LCASE(?name) = "${domainId.toLowerCase()}") } } LIMIT 1`);
  return r2.results.bindings.length > 0 ? r2.results.bindings[0].d.value : null;
}

// POST /api/icd/domains/:id/fields
app.post('/api/icd/domains/:id/fields', async (req: Request, res: Response) => {
  try {
    const { name, severity, datatype, constraint, cardinality, bestSource, description, order } = req.body;
    if (!name || !severity) { res.status(400).json({ error: 'name and severity are required' }); return; }
    const validSev = ['violation', 'enrichment', 'warning', 'info'];
    if (!validSev.includes(severity)) { res.status(400).json({ error: `severity must be one of: ${validSev.join(', ')}` }); return; }

    const domainUri = await resolveIcdDomain(req.params.id);
    if (!domainUri) { res.status(404).json({ error: `Domain '${req.params.id}' not found` }); return; }

    const slug = icdSlug(req.params.id);
    const fieldSlug = icdSlug(name);
    const fieldUri = `https://jeffbridwell.com/icd/field/${slug}/${fieldSlug}`;
    const typeUri = `https://jeffbridwell.com/icd/type/${slug}`;
    const sevMap: Record<string, string> = { violation: 'icd:Violation', enrichment: 'icd:Enrichment', warning: 'icd:Warning', info: 'icd:Info' };

    const exists = await icdSparqlQuery(`${ICD_PFX} SELECT ?f WHERE { GRAPH <${ICD_GRAPH}> { <${fieldUri}> a icd:CanonicalField } } LIMIT 1`);
    const isNew = exists.results.bindings.length === 0;

    await icdSparqlUpdate(`${ICD_PFX}
      DELETE WHERE { GRAPH <${ICD_GRAPH}> { <${fieldUri}> ?p ?o } };
      INSERT DATA { GRAPH <${ICD_GRAPH}> {
        <${fieldUri}> a icd:CanonicalField ;
          icd:canonicalName "${escSparql(name)}" ; icd:displayName "${escSparql(name)}" ;
          icd:severity ${sevMap[severity]} ; icd:datatype "${escSparql(datatype || 'xsd:string')}" ;
          icd:cardinality "${escSparql(cardinality || '1')}" ; icd:fieldOrder ${order ?? 0} ;
          icd:inDomain <${domainUri}> ; icd:inConsumerType <${typeUri}> .
        ${constraint ? `<${fieldUri}> icd:constraint "${escSparql(constraint)}" .` : ''}
        ${bestSource ? `<${fieldUri}> icd:bestSource "${escSparql(bestSource)}" .` : ''}
        ${description ? `<${fieldUri}> icd:fieldTypeDescription "${escSparql(description)}" .` : ''}
        <${typeUri}> icd:hasCanonicalField <${fieldUri}> .
      } }`);

    res.status(isNew ? 201 : 200).json({ ok: true, domain: req.params.id, field: name, created: isNew });
  } catch (err) {
    res.status(500).json({ error: 'Failed to upsert ICD field', detail: String(err) });
  }
});

// POST /api/icd/domains/:id/mappings
app.post('/api/icd/domains/:id/mappings', async (req: Request, res: Response) => {
  try {
    const { providerId, sourceField, mapsTo, confidence, transform, description, coverageLabel, coverageClass, order } = req.body;
    if (!providerId || !sourceField || !mapsTo || !confidence) {
      res.status(400).json({ error: 'providerId, sourceField, mapsTo, and confidence are required' }); return;
    }

    const domainUri = await resolveIcdDomain(req.params.id);
    if (!domainUri) { res.status(404).json({ error: `Domain '${req.params.id}' not found` }); return; }

    const slug = icdSlug(req.params.id);
    const provSlug = icdSlug(providerId);
    const provUri = `https://jeffbridwell.com/icd/provider/${slug}/${provSlug}`;
    const mappingSlug = icdSlug(sourceField);
    const mappingUri = `https://jeffbridwell.com/icd/mapping/${slug}/${mappingSlug}`;
    const mapsToSlug = icdSlug(mapsTo.split(',')[0].trim());
    const fieldUri = `https://jeffbridwell.com/icd/field/${slug}/${mapsToSlug}`;

    const provExists = await icdSparqlQuery(`${ICD_PFX} SELECT ?p WHERE { GRAPH <${ICD_GRAPH}> { <${provUri}> a icd:Provider } } LIMIT 1`);
    if (provExists.results.bindings.length === 0) { res.status(404).json({ error: `Provider '${providerId}' not found` }); return; }

    const exists = await icdSparqlQuery(`${ICD_PFX} SELECT ?m WHERE { GRAPH <${ICD_GRAPH}> { <${mappingUri}> a icd:FieldMapping } } LIMIT 1`);
    const isNew = exists.results.bindings.length === 0;

    await icdSparqlUpdate(`${ICD_PFX}
      DELETE WHERE { GRAPH <${ICD_GRAPH}> { <${mappingUri}> ?p ?o } };
      INSERT DATA { GRAPH <${ICD_GRAPH}> {
        <${mappingUri}> a icd:FieldMapping ;
          icd:mappingOrder ${order ?? 0} ; icd:sourceTable "${escSparql(providerId)}" ;
          icd:sourceField "${escSparql(sourceField)}" ; icd:mapsTo <${fieldUri}> ;
          icd:mapsToName "${escSparql(mapsTo)}" ; icd:confidence "${escSparql(confidence)}" ;
          icd:fromProvider <${provUri}> .
        ${transform ? `<${mappingUri}> icd:transform "${escSparql(transform)}" .` : ''}
        ${description ? `<${mappingUri}> icd:fieldDescription "${escSparql(description)}" .` : ''}
        ${coverageLabel ? `<${mappingUri}> icd:fieldCoverageLabel "${escSparql(coverageLabel)}" .` : ''}
        ${coverageClass ? `<${mappingUri}> icd:fieldCoverageClass "${escSparql(coverageClass)}" .` : ''}
        <${provUri}> icd:hasMapping <${mappingUri}> .
      } }`);

    res.status(isNew ? 201 : 200).json({ ok: true, domain: req.params.id, provider: providerId, sourceField, created: isNew });
  } catch (err) {
    res.status(500).json({ error: 'Failed to upsert ICD mapping', detail: String(err) });
  }
});

// PUT /api/icd/domains/:id/providers/:pid/sections
app.put('/api/icd/domains/:id/providers/:pid/sections', async (req: Request, res: Response) => {
  try {
    const { title, type, paragraphs, risks, nonFunctionals, mermaid } = req.body;
    if (!title) { res.status(400).json({ error: 'title is required' }); return; }

    const domainUri = await resolveIcdDomain(req.params.id);
    if (!domainUri) { res.status(404).json({ error: `Domain '${req.params.id}' not found` }); return; }

    const slug = icdSlug(req.params.id);
    const provSlug = icdSlug(req.params.pid);
    const secSlug = icdSlug(title);
    const provUri = `https://jeffbridwell.com/icd/provider/${slug}/${provSlug}`;
    const secUri = `https://jeffbridwell.com/icd/section/${slug}/${secSlug}`;

    const provExists = await icdSparqlQuery(`${ICD_PFX} SELECT ?p WHERE { GRAPH <${ICD_GRAPH}> { <${provUri}> a icd:Provider } } LIMIT 1`);
    if (provExists.results.bindings.length === 0) { res.status(404).json({ error: `Provider '${req.params.pid}' not found` }); return; }

    // Delete existing section + sub-resources
    await icdSparqlUpdate(`${ICD_PFX}
      DELETE WHERE { GRAPH <${ICD_GRAPH}> { <${secUri}> icd:hasParagraph ?para . ?para ?pp ?po . } };
      DELETE WHERE { GRAPH <${ICD_GRAPH}> { <${secUri}> icd:hasRiskItem ?risk . ?risk ?rp ?ro . } };
      DELETE WHERE { GRAPH <${ICD_GRAPH}> { <${secUri}> ?p ?o } }`);

    // Build section triples
    const sType = type || 'content';
    let triples = `<${secUri}> a icd:Section ; icd:sectionTitle "${escSparql(title)}" ; icd:sectionType "${escSparql(sType)}" ; icd:sectionOrder 0 . <${provUri}> icd:hasSection <${secUri}> .`;

    if (paragraphs) {
      for (let i = 0; i < paragraphs.length; i++) {
        const pUri = `${secUri}/para-${i}`;
        triples += ` <${secUri}> icd:hasParagraph <${pUri}> . <${pUri}> a icd:Paragraph ; icd:paragraphOrder ${i} ; icd:paragraphLabel "" ; icd:paragraphText "${escSparql(paragraphs[i])}" .`;
      }
    }
    if (risks) {
      for (let i = 0; i < risks.length; i++) {
        const rUri = `${secUri}/risk-${i}`;
        triples += ` <${secUri}> icd:hasRiskItem <${rUri}> . <${rUri}> a icd:RiskItem ; icd:riskOrder ${i} ; icd:riskStatus "${escSparql(risks[i].status)}" ; icd:riskText "${escSparql(risks[i].text)}" .`;
      }
    }
    if (nonFunctionals) {
      const nf = nonFunctionals;
      triples += ` <${secUri}> icd:nfVolume "${escSparql(nf.volume)}" ; icd:nfFreshness "${escSparql(nf.freshness)}" ; icd:nfLatency "${escSparql(nf.latency)}" ; icd:nfAuth "${escSparql(nf.auth)}" .`;
    }
    if (mermaid) {
      triples += ` <${secUri}> icd:mermaidSource """${mermaid}""" .`;
    }

    await icdSparqlUpdate(`${ICD_PFX} INSERT DATA { GRAPH <${ICD_GRAPH}> { ${triples} } }`);
    res.json({ ok: true, domain: req.params.id, provider: req.params.pid, section: title });
  } catch (err) {
    res.status(500).json({ error: 'Failed to update ICD section', detail: String(err) });
  }
});

// --- Error handler ---

app.use((err: Error, _req: Request, res: Response, _next: NextFunction) => {
  console.error(`[chorus-api] Error: ${err.message}`);
  res.status(500).json({ error: 'Internal server error' });
});

// --- Start ---

// --- GET /api/chorus/domain/:name — Full domain view (#1908) ---

const DOMAIN_REGISTRY: Record<string, { product: string; step: string; description: string }> = {
  photos:    { product: 'gathering', step: 'harvesting', description: 'Three eras, 63K canonical from 83K source. Apple + Google + Takeout.' },
  music:     { product: 'gathering', step: 'harvesting', description: '40+ years, 100K tracks. Apple Music harvest. Shuffle algorithms.' },
  people:    { product: 'gathering', step: 'harvesting', description: '3,942 contacts, 48 face clusters. LinkedIn, Facebook, Apple, Google exports.' },
  stories:   { product: 'gathering', step: 'reflecting', description: 'Narrative capture. Manual + voice memo + seed pipeline.' },
  documents: { product: 'gathering', step: 'practicing', description: 'Reading list. Done/reading/to-read status tracking.' },
  social:    { product: 'gathering', step: 'harvesting', description: 'Archive from Facebook/LinkedIn GDPR exports. Static, one-time.' },
  notes:     { product: 'gathering', step: 'practicing', description: 'Quick capture. Downstream from seeds, upstream to stories.' },
  webmethods:{ product: 'chorus',    step: 'building', description: 'Reference data. OAGIS verb mapping for ICD integration patterns.' },
  seeds:     { product: 'gathering', step: 'sowing', description: 'SMS intake from phone. Two-message pattern (content + #hashtag).' },
  glimmers:  { product: 'gathering', step: 'sowing', description: 'Sparks noticed but not committed. Ignite or fade.' },
  ideas:     { product: 'gathering', step: 'growing', description: 'Ideas promote to Projects. Native CRUD. Sharing model.' },
  property:  { product: 'gathering', step: 'practicing', description: 'Houses, rooms, gardens, beds. Nested hierarchy.' },
  cooking:   { product: 'gathering', step: 'practicing', description: 'Recipes and food. Tag filtering. #cooking seed routing.' },
  reading:   { product: 'gathering', step: 'practicing', description: 'Reading list. Status tracking. #read routing.' },
  watching:  { product: 'gathering', step: 'practicing', description: 'Movies, shows, videos. Star ratings, category + status filters.' },
  books:     { product: 'gathering', step: 'harvesting', description: '141+ items. Photo upload with Claude Vision classification.' },
  gallery:   { product: 'gathering', step: 'harvesting', description: 'Tag filtering. Lightbox viewer. OG link previews.' },
  blog:      { product: 'gathering', step: 'reflecting', description: 'WordPress at 192.168.86.36:8081. Harvested via REST API.' },
  self:      { product: 'gathering', step: 'reflecting', description: 'Jeff\'s self domain. Ontology from spring 2024 sketch.' },
  search:    { product: 'gathering', step: 'practicing', description: 'Full-text search across all domains. Semantic embeddings.' },
  chorus:    { product: 'chorus',    step: 'building', description: 'Team coordination product. Hooks, gates, pulse, Clearing.' },
  infrastructure: { product: 'chorus', step: 'building', description: 'Servers, LaunchAgents, deploy, disk, network. Two machines.' },
  'knowledge-graph': { product: 'gathering', step: 'practicing', description: 'RDF/SPARQL semantic layer. Fuseki, ontologies, SHACL validation.' },
  observability: { product: 'chorus', step: 'building', description: 'Grafana, Loki, Promtail, alerts. Operational visibility.' },
  loom:      { product: 'chorus',    step: 'directing', description: 'Team coordination surface. Roles, cards, briefs, decisions.' },
};

// #2175: domain endpoint is on the envelope hot path (chorus-hooks Rust helper
// calls it every prompt with a 500ms timeout). Section queries + completeness
// + board filter spike to >1s cold. Cache full response for 60s — same shape
// as the existing boardCache / healthCache patterns in this file.
const domainResponseCache = new Map<string, { body: any; ts: number }>();
const DOMAIN_CACHE_TTL_MS = 60 * 1000;

app.get('/api/chorus/domain/:name', async (_req: Request, res: Response) => {
  const name = _req.params.name.toLowerCase();
  const meta = DOMAIN_REGISTRY[name];
  if (!meta) {
    res.status(404).json({ error: `Unknown domain: ${name}`, validDomains: Object.keys(DOMAIN_REGISTRY) });
    return;
  }

  const cached = domainResponseCache.get(name);
  if (cached && Date.now() - cached.ts < DOMAIN_CACHE_TTL_MS) {
    res.json(cached.body);
    return;
  }

  try {
    const boardTs = `${CHORUS_ROOT}/platform/scripts/cards`;
    const envOpts = {
      encoding: 'utf-8' as const, timeout: 15000,
      env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
    };

    // #2096: read from board cache instead of shelling out (was ~450ms, now <1ms)
    const cards = getBoardCards()
      .filter(c => c.status !== 'Done' && c.status !== "Won't Do" && c.tags.includes(`domain:${name}`))
      .map(c => ({ id: c.id, title: c.title, status: c.status, owner: c.owner, type: c.type }));

    const wip = cards.filter(c => c.status === 'WIP');
    const blocked = cards.filter(c => c.status === 'Blocked');

    // Parse domain HTML page for structured sections
    const fs = require('fs');
    const domainHtmlPath = `${CHORUS_ROOT}/platform/roles/product-manager/artifacts/domain-${name}.html`;
    let sections: Record<string, any> = {};
    try {
      if (fs.existsSync(domainHtmlPath)) {
        const html = fs.readFileSync(domainHtmlPath, 'utf-8');
        // Split by h2, extract section name and table rows
        const h2Parts = html.split(/<h2>/);
        for (const part of h2Parts.slice(1)) {
          const titleMatch = part.match(/^([^<]+)<\/h2>/);
          if (!titleMatch) continue;
          const sectionName = titleMatch[1].trim().toLowerCase().replace(/\s+/g, '_');

          // Extract table rows as arrays
          const rows: string[][] = [];
          const trMatches = part.match(/<tr>([\s\S]*?)<\/tr>/g) || [];
          for (const tr of trMatches) {
            const cells = (tr.match(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/g) || [])
              .map((cell: string) => cell.replace(/<[^>]+>/g, '').trim());
            if (cells.length > 0) rows.push(cells);
          }

          // Extract list items
          const listItems = (part.match(/<li[^>]*>([\s\S]*?)<\/li>/g) || [])
            .map((li: string) => li.replace(/<[^>]+>/g, '').trim())
            .filter((s: string) => s.length > 0);

          sections[sectionName] = {
            title: titleMatch[1].trim(),
            table: rows.length > 0 ? rows : undefined,
            items: listItems.length > 0 ? listItems : undefined,
          };
        }
      }
    } catch {}

    // Fetch completeness from Athena if subdomain exists (#1899)
    let completeness: any = null;
    let subdomainId: string | null = null;
    try {
      const sdId = `${name}-service`;
      const cRes = await fetch(`http://localhost:3340/api/athena/subdomains/${sdId}/completeness`);
      if (cRes.ok) {
        const cBody = await cRes.json() as any;
        completeness = cBody.data;
        subdomainId = sdId;
      } else {
        // Try domain suffix
        const cRes2 = await fetch(`http://localhost:3340/api/athena/subdomains/${name}-domain/completeness`);
        if (cRes2.ok) {
          const cBody2 = await cRes2.json() as any;
          completeness = cBody2.data;
          subdomainId = `${name}-domain`;
        }
      }
    } catch {}

    // #2175: if sections empty (no legacy HTML page) and we have a subdomain,
    // pull section labels from Fuseki via per-predicate parallel queries.
    if (subdomainId && Object.keys(sections).length === 0) {
      const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;
      const sectionPreds: Array<[string, string]> = [
        ['scenarios', 'hasScenario'],
        ['contract', 'hasContract'],
        ['prior_art', 'hasPriorArt'],
        ['integrations', 'hasIntegration'],
        ['services', 'hasService'],
        ['persistence', 'hasPersistence'],
        ['pipeline', 'hasPipeline'],
        ['gaps', 'hasGap'],
        ['actors', 'hasActor'],
        ['pages', 'hasPage'],
        ['logs', 'hasLogSource'],
      ];
      try {
        // #2178 — surface ownership + description + reads/writes/consumes
        // alongside labels. items string[] preserved for existing consumers;
        // itemDetails adds provenance. Ownership walks up from sub-domain
        // when the entity doesn't carry chorus:ownedBy directly (Silas's
        // call: COALESCE(direct, parent); services get re-parented, direct
        // tags drift when sub-domain ownership changes). ownerInherited:true
        // flags walk-up-sourced owner so consumers can distinguish.
        const parentOwnerQuery = `
          PREFIX chorus: <https://jeffbridwell.com/chorus#>
          PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
          SELECT ?ownerLabel WHERE {
            GRAPH <urn:chorus:ontology> {
              <${sdUri}> chorus:ownedBy ?owner .
              OPTIONAL { ?owner rdfs:label ?ownerLabel }
            }
          } LIMIT 1
        `;
        const parentOwnerResult = await athenaSparqlQuery(parentOwnerQuery).catch(() => null);
        const parentOwner: string | null =
          parentOwnerResult?.results?.bindings?.[0]?.ownerLabel?.value || null;

        const sectionQuery = (pred: string) => `
          PREFIX chorus: <https://jeffbridwell.com/chorus#>
          PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
          SELECT ?e ?label ?comment
                 (GROUP_CONCAT(DISTINCT ?ownerLabel; separator="||") AS ?owners)
                 (GROUP_CONCAT(DISTINCT ?readLabel; separator="||") AS ?reads)
                 (GROUP_CONCAT(DISTINCT ?writeLabel; separator="||") AS ?writes)
                 (GROUP_CONCAT(DISTINCT ?consumesLabel; separator="||") AS ?consumes)
          WHERE {
            GRAPH <urn:chorus:instances> {
              <${sdUri}> chorus:${pred} ?e .
              OPTIONAL { ?e rdfs:label ?label }
              OPTIONAL { ?e rdfs:comment ?comment }
              OPTIONAL { ?e chorus:ownedBy ?ownerEnt . OPTIONAL { ?ownerEnt rdfs:label ?ownerLabel } }
              OPTIONAL { ?e chorus:reads ?readTarget . OPTIONAL { ?readTarget rdfs:label ?readLabel } }
              OPTIONAL { ?e chorus:writes ?writeTarget . OPTIONAL { ?writeTarget rdfs:label ?writeLabel } }
              OPTIONAL { ?e chorus:consumes ?consumesTarget . OPTIONAL { ?consumesTarget rdfs:label ?consumesLabel } }
            }
          }
          GROUP BY ?e ?label ?comment
          LIMIT 20
        `;
        const results = await Promise.all(
          sectionPreds.map(([, pred]) => athenaSparqlQuery(sectionQuery(pred)).catch(() => null))
        );
        sectionPreds.forEach(([key], i) => {
          const r = results[i];
          if (!r?.results?.bindings?.length) return;
          const items: string[] = [];
          const itemDetails: any[] = [];
          for (const b of r.results.bindings as any[]) {
            const label = b?.label?.value;
            if (!label) continue;
            items.push(label);
            const split = (v: string | undefined) => (v ? v.split('||').filter(Boolean) : []);
            const detail: any = { label };
            if (b?.comment?.value) detail.description = b.comment.value;
            const directOwners = split(b?.owners?.value);
            const reads = split(b?.reads?.value);
            const writes = split(b?.writes?.value);
            const consumes = split(b?.consumes?.value);
            // COALESCE(direct, parent): direct owner takes precedence for
            // genuine co-ownership cases; sub-domain walk-up is the default.
            if (directOwners.length) {
              detail.owner = directOwners.length === 1 ? directOwners[0] : directOwners;
            } else if (parentOwner) {
              detail.owner = parentOwner;
              detail.ownerInherited = true;
            }
            if (reads.length) detail.reads = reads;
            if (writes.length) detail.writes = writes;
            if (consumes.length) detail.consumes = consumes;
            itemDetails.push(detail);
          }
          if (items.length > 0) {
            sections[key] = { title: key.replace(/_/g, ' '), items, itemDetails };
          }
        });
      } catch {}
    }

    if (completeness && completeness.percentage < 60) {
      console.warn(`[domain-completeness] ${name}: ${completeness.percentage}% — missing: ${completeness.missing?.join(', ')}`);
    }

    const body = {
      domain: name,
      product: meta.product,
      step: meta.step,
      description: meta.description,
      sections,
      cards: {
        total: cards.length,
        wip: wip.length,
        blocked: blocked.length,
        items: cards,
      },
      completeness: completeness ? {
        percentage: completeness.percentage,
        present: completeness.present,
        missing: completeness.missing,
        lifecycle: completeness.lifecycle,
      } : null,
      hasIcd: ['photos', 'stories', 'people', 'music', 'documents', 'social', 'notes', 'webmethods'].includes(name),
      icdEndpoint: `/api/icd/domains/${name}`,
    };
    domainResponseCache.set(name, { body, ts: Date.now() });
    res.json(body);
  } catch (error) {
    res.status(500).json({ error: 'Failed to build domain view', detail: error instanceof Error ? error.message : String(error) });
  }
});

// --- GET /api/chorus/domains — List all domains (#1908) ---
app.get('/api/chorus/domains', (_req: Request, res: Response) => {
  const domains = Object.entries(DOMAIN_REGISTRY).map(([name, meta]) => ({
    name,
    ...meta,
    hasIcd: ['photos', 'stories', 'people', 'music', 'documents', 'social', 'notes', 'webmethods'].includes(name),
  }));
  res.json({ domains, total: domains.length });
});

// --- GET /api/chorus/health (#2011, #1978 cache) ---

const startTime = Date.now();

// Cache expensive counts — refresh every 30s, serve from cache (#1978)
let healthCache = { dbRows: 0, unembedded: 0, vectors: 0, dbStatus: 'unknown', hooksStatus: 'unknown', ts: 0 };

async function refreshHealthCache(): Promise<void> {
  try {
    const db = new Database(DB_PATH, { readonly: true });
    const row = db.prepare('SELECT COUNT(*) as cnt FROM messages').get() as { cnt: number };
    healthCache.dbRows = row.cnt;
    healthCache.dbStatus = 'ok';
    try {
      const uRow = db.prepare('SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= 100').get() as { cnt: number };
      healthCache.unembedded = uRow.cnt;
    } catch { /* column may not exist yet */ }
    db.close();
  } catch {
    healthCache.dbStatus = 'error';
  }
  try {
    if (lanceTable) healthCache.vectors = await lanceTable.countRows();
  } catch { /* non-fatal */ }
  const hookBinary = path.resolve(__dirname, '../../services/chorus-hooks/target/release/chorus-hooks');
  try {
    if (fs.existsSync(hookBinary)) {
      const stat = fs.statSync(hookBinary);
      healthCache.hooksStatus = (Date.now() - stat.mtimeMs) / 3600000 < 24 ? 'active' : 'stale';
    } else { healthCache.hooksStatus = 'missing'; }
  } catch { healthCache.hooksStatus = 'error'; }
  healthCache.ts = Date.now();
}

// Scheduled reindex — keep index_freshness sources current (#1960)
// indexAllSources() is SQLite-only (no Ollama), safe in-process unlike embedDelta (#1978).
// Runs every 15 min. First run after 60s startup delay to avoid boot contention.
// The timer starts are at the bottom of this file inside `require.main === module`
// (#2173 AC4) — fn declaration lives here so the start can reference it.
const REINDEX_INTERVAL = 15 * 60_000;
let reindexRunning = false;
async function scheduledReindex(): Promise<void> {
  if (reindexRunning) return;
  reindexRunning = true;
  try {
    const result = await indexAllSources();
    const total = Object.values(result).filter(v => typeof v === 'string' && v.startsWith('indexed')).length;
    console.log(`[reindex] scheduled run complete — ${total} sources indexed`);
  } catch (err: any) {
    console.error(`[reindex] scheduled run failed: ${err.message}`);
  } finally {
    reindexRunning = false;
  }
}

// SHACL validation — check ontology integrity (#2014).
// Extracted to handlers/athena-validate.ts (#2180).
app.get('/api/athena/validate', async (_req: Request, res: Response) => {
  const r = await fetchAthenaValidate({ sparql: athenaSparqlQuery, timestamp: bostonNow });
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/health', (_req: Request, res: Response) => {
  // Liveness + uptime — no expensive queries (#1978)
  const uptime = Math.floor((Date.now() - startTime) / 1000);
  res.json({ status: 'healthy', uptime, timestamp: bostonNow() });
});

// Health cache exposed via /api/chorus/health/detail for deep-health (#1978)
app.get('/api/chorus/health/detail', async (_req: Request, res: Response) => {
  // Ollama availability check (#1980)
  let ollamaStatus = 'unknown';
  try {
    const ollamaRes = await fetch(`${OLLAMA_URL}/api/tags`, { signal: AbortSignal.timeout(3000) });
    ollamaStatus = ollamaRes.ok ? 'up' : 'degraded';
  } catch { ollamaStatus = 'down'; }

  res.json({
    db: { status: healthCache.dbStatus, rows: healthCache.dbRows },
    vectors: healthCache.vectors,
    unembedded: healthCache.unembedded,
    hooks: { status: healthCache.hooksStatus },
    ollama: { status: ollamaStatus },
    timestamp: bostonNow(),
  });
});

// --- GET /api/chorus/hooks/metrics (#2277) ---

let hooksMetricsCache: { data: any; ts: number } | null = null;
const HOOKS_CACHE_TTL = 60_000; // 60s

import { fetchChorusHooksMetrics } from './handlers/chorus-hooks-metrics';
app.get('/api/chorus/hooks/metrics', (_req: Request, res: Response) => {
  if (hooksMetricsCache && (Date.now() - hooksMetricsCache.ts) < HOOKS_CACHE_TTL) {
    res.json(hooksMetricsCache.data);
    return;
  }
  const HOOKS_LOG = path.join(os.homedir(), 'Library/Logs/Gathering/hooks.log');
  const r = fetchChorusHooksMetrics({
    readLog: () => fs.existsSync(HOOKS_LOG) ? fs.readFileSync(HOOKS_LOG, 'utf-8') : null,
  });
  if (r.status === 200) hooksMetricsCache = { data: r.body, ts: Date.now() };
  res.status(r.status).json(r.body);
});

// --- Crash handlers: log + alert before dying ---
const NUDGE_PATH = path.resolve(__dirname, '../../scripts/nudge');

function crashAlert(reason: string): void {
  // Log only — no nudges, no notifications. Silas checks the log at session start.
  console.error(`[chorus-api] CRASH LOGGED: ${reason}`);
}

// ── Athena CMDB API ──────────────────────────────────────────────
// Named SPARQL queries against the Chorus ontology in Fuseki.
// Access layer for agents — no raw SPARQL, no port guessing.

// CORS for Athena — allows pages on localhost:3000 to fetch from 3340
app.use('/api/athena', (_req: Request, res: Response, next: NextFunction) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type');
  if (_req.method === 'OPTIONS') return res.sendStatus(204);
  next();
});

const ATHENA_GRAPH = 'urn:chorus:ontology';
const ATHENA_INSTANCES = 'urn:chorus:instances';
const ATHENA_SPARQL = 'http://localhost:3030/pods/sparql';
const SPARQL_DIR = path.resolve(__dirname, 'sparql');

const ATHENA_QUERIES = [
  { name: 'health', path: '/api/athena/health', description: 'Ontology health — triple count, endpoint status' },
  { name: 'products', path: '/api/athena/products', description: 'List all products' },
  { name: 'subproducts', path: '/api/athena/subproducts', description: 'List sub-products with owner, domain count, consumes count' },
  { name: 'subdomains', path: '/api/athena/subdomains', description: 'List sub-domains with owner, step. Filter: ?owner, ?step' },
  { name: 'blast-radius', path: '/api/athena/subdomains/:id/blast-radius', description: 'Which sub-products consume a given sub-domain' },
  { name: 'steps', path: '/api/athena/steps', description: 'Value stream steps with sub-domains at each step' },
  { name: 'owners', path: '/api/athena/owners', description: 'Owners with sub-domain counts' },
  { name: 'machines', path: '/api/athena/machines', description: 'Machines with running services' },
];

function loadSparql(name: string): string {
  return fs.readFileSync(path.join(SPARQL_DIR, `${name}.sparql`), 'utf-8').trim();
}

async function athenaSparqlQuery(query: string): Promise<any> {
  const res = await fetch(ATHENA_SPARQL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/sparql-query', 'Accept': 'application/sparql-results+json' },
    body: query,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Fuseki ${res.status}: ${text.slice(0, 200)}`);
  }
  return res.json();
}

const ATHENA_UPDATE = 'http://localhost:3030/pods/update';

async function athenaSparqlUpdate(update: string): Promise<void> {
  const res = await fetch(ATHENA_UPDATE, {
    method: 'POST',
    headers: { 'Content-Type': 'application/sparql-update' },
    body: update,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Fuseki update ${res.status}: ${text.slice(0, 200)}`);
  }
}

function athenaEnvelope(queryName: string, data: any, durationMs: number, extra: Record<string, any> = {}) {
  return {
    _meta: { source: 'athena', query_name: queryName, graph: ATHENA_GRAPH, duration_ms: durationMs, cached: false, timestamp: bostonNow(), ...extra },
    data,
  };
}

// GET /api/athena/health — discovery endpoint, lists available queries
// Extracted to handlers/athena-health.ts (#2173 AC4). SPARQL client + query
// loader are injected so unit tests run without Fuseki.
import { fetchAthenaHealth } from './handlers/athena-health';
import { fetchAthenaValidate } from './handlers/athena-validate';
import { fetchAthenaProducts } from './handlers/athena-products';
import { fetchAthenaSubproducts } from './handlers/athena-subproducts';
import { fetchAthenaSteps } from './handlers/athena-steps';
import { fetchAthenaOwners } from './handlers/athena-owners';
import { fetchAthenaMachines } from './handlers/athena-machines';
import { fetchAthenaSubdomains } from './handlers/athena-subdomains';
import { fetchAthenaSubdomainDetail } from './handlers/athena-subdomain-detail';
import { fetchAthenaBlastRadius } from './handlers/athena-blast-radius';
import { fetchAthenaSubdomainCards } from './handlers/athena-subdomain-cards';
import { fetchAthenaSubdomainCode } from './handlers/athena-subdomain-code';
import { fetchAthenaSubdomainAlerts } from './handlers/athena-subdomain-alerts';
import { fetchAthenaSubdomainCoverage, fetchAthenaSubdomainTestCoverage } from './handlers/athena-subdomain-coverage';
import { fetchAthenaSubdomainPages } from './handlers/athena-subdomain-pages';
import { fetchAthenaSubdomainEndpoints } from './handlers/athena-subdomain-endpoints';
import {
  fetchAthenaSubdomainActors,
  fetchAthenaSubdomainScenarios,
  fetchAthenaSubdomainContract,
  fetchAthenaSubdomainIntegrations,
  fetchAthenaSubdomainPersistence,
  fetchAthenaSubdomainPriorArt,
} from './handlers/athena-subdomain-facets';
import { fetchAthenaSubdomainCompleteness } from './handlers/athena-subdomain-completeness';
import { fetchAthenaCardDetail } from './handlers/athena-card-detail';
app.get('/api/athena/health', async (_req: Request, res: Response) => {
  const r = await fetchAthenaHealth({
    sparql: athenaSparqlQuery,
    loadQuery: loadSparql,
    envelope: athenaEnvelope,
  });
  res.status(r.status).json(r.body);
});

// GET /api/athena/products — list all products
app.get('/api/athena/products', async (_req: Request, res: Response) => {
  const r = await fetchAthenaProducts({ sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// GET /api/chorus/products — full product hierarchy: products → subproducts → subdomains (#2093, extracted #2189)
import { fetchChorusProducts } from './handlers/chorus-products';
app.get('/api/chorus/products', async (_req: Request, res: Response) => {
  const r = await fetchChorusProducts({ sparql: athenaSparqlQuery });
  res.status(r.status).json(r.body);
});

// GET /api/athena/subproducts — list sub-products with owner, domain count, consumes count
app.get('/api/athena/subproducts', async (_req: Request, res: Response) => {
  const r = await fetchAthenaSubproducts({ sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains — list sub-domains with owner, step. Filter: ?owner, ?step
app.get('/api/athena/subdomains', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomains(
    { sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope },
    { owner: req.query.owner as string | undefined, step: req.query.step as string | undefined },
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/blast-radius — what breaks if this sub-domain fails
app.get('/api/athena/subdomains/:id/blast-radius', async (req: Request, res: Response) => {
  const r = await fetchAthenaBlastRadius(
    { sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id — single sub-domain detail
app.get('/api/athena/subdomains/:id', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainDetail(
    { sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/steps — value stream steps with sub-domains at each step
app.get('/api/athena/steps', async (_req: Request, res: Response) => {
  const r = await fetchAthenaSteps({ sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// GET /api/athena/owners — owners with sub-domain counts
app.get('/api/athena/owners', async (_req: Request, res: Response) => {
  const r = await fetchAthenaOwners({ sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// GET /api/athena/machines — machines with running services
app.get('/api/athena/machines', async (_req: Request, res: Response) => {
  const r = await fetchAthenaMachines({ sparql: athenaSparqlQuery, loadQuery: loadSparql, envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/cards — active board cards for this domain
app.get('/api/athena/subdomains/:id/cards', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainCards(
    { getBoardCards, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/alerts — alert rules related to this domain
app.get('/api/athena/subdomains/:id/alerts', async (req: Request, res: Response) => {
  const ALERTS_DIR = path.join(REPO_ROOT, 'proving/domains/alerts');
  const r = await fetchAthenaSubdomainAlerts(
    {
      listAlertFiles: () => fs.readdirSync(ALERTS_DIR).filter((f: string) => f.endsWith('.yml')),
      readAlertFile: (f: string) => fs.readFileSync(path.join(ALERTS_DIR, f), 'utf-8'),
      envelope: athenaEnvelope,
    },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/code — code inventory from instances graph (#1868)
app.get('/api/athena/subdomains/:id/code', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainCode(
    { sparql: athenaSparqlQuery, extname: path.extname, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/code — add code file to subdomain (#1868)
app.post('/api/athena/subdomains/:id/code', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, path: filePath, type: fileType, description } = req.body || {};
    if (!filePath && !label) return res.status(400).json(athenaEnvelope('subdomain-code-create', { error: 'Missing required field: path or label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const name = label || filePath;
    const fileId = `${req.params.id}-code-${name.replace(/[\/\.]/g, '-').toLowerCase()}`;
    const fileUri = `https://jeffbridwell.com/chorus#${fileId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${fileUri}> a chorus:CodeFile ; rdfs:label "${name.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasCodeFile <${fileUri}> . ${filePath ? `<${fileUri}> chorus:filePath "${filePath.replace(/"/g, '\\"')}" .` : ''} ${fileType ? `<${fileUri}> chorus:fileType "${fileType}" .` : ''} ${description ? `<${fileUri}> rdfs:comment "${description.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-code-create', { subdomain: req.params.id, uri: fileUri, label: name, path: filePath || null, type: fileType || null, description: description || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-code-create', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/discover-code — auto-discover code files per domain from filesystem (#1868 AC1)
app.post('/api/athena/discover-code', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    // 1. Get all SubDomains from ontology
    const sdQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }`;
    const sdResult = await athenaSparqlQuery(sdQuery);
    const domains = sdResult.results.bindings.map((b: any) => ({
      id: b.sd.value.split('#').pop() as string,
      label: b.label.value as string,
    }));

    // 2. Domain alias map: ontology id → filesystem name patterns
    // Skip domains whose base name is too generic (matches nearly every file)
    const genericBases = new Set(['services', 'service', 'domains', 'domain', 'code', 'loom', 'time', 'streams', 'stream', 'messages', 'message', 'policies', 'policy']);
    const aliasMap: Record<string, string[]> = {};
    for (const d of domains) {
      const base = d.id.replace(/-(domain|service)$/, '');
      if (genericBases.has(base)) continue;
      const aliases = [base];
      if (base.endsWith('s') && !base.endsWith('ss')) {
        if (base.endsWith('ies')) aliases.push(base.replace(/ies$/, 'y'));
        else aliases.push(base.replace(/s$/, ''));
      }
      aliasMap[d.id] = aliases;
    }
    // Special cases not derivable from the label
    aliasMap['blog-domain'] = ['blog', 'wordpress'];
    aliasMap['social-domain'] = ['social', 'socialpost'];
    aliasMap['people-domain'] = ['people', 'person'];
    aliasMap['documents-domain'] = ['documents', 'document', 'doc-catalog'];
    aliasMap['knowledge-domain'] = ['knowledge', 'knowledge-graph'];
    aliasMap['sexuality-domain'] = ['sexuality', 'self-ai'];
    aliasMap['seeds-domain'] = ['seeds', 'seed', 'sms-seed'];
    aliasMap['convergence-domain'] = ['convergence', 'ontology'];
    aliasMap['chorus-domain'] = ['chorus', 'clearing', 'bridge', 'context-cache'];
    aliasMap['infra-service'] = ['infrastructure', 'infra', 'app-state', 'agent-state'];
    aliasMap['observability-service'] = ['observability', 'dashboard'];
    aliasMap['cards-service'] = ['cards', 'board'];
    aliasMap['skills-service'] = ['skills'];
    aliasMap['gates-service'] = ['gates', 'gate'];
    aliasMap['spine-service'] = ['spine', 'chorus-log'];
    aliasMap['logs-service'] = ['logs', 'log-freshness'];
    aliasMap['alerts-service'] = ['alerts', 'alert'];
    aliasMap['deploys-service'] = ['deploys', 'deploy', 'app-state'];

    // 3. Scan source trees
    const GATHERING_ROOT = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site');
    const CHORUS_ROOT = path.resolve(__dirname, '../../..');
    const discovered: Array<{ domainId: string; filePath: string; fileType: string }> = [];

    // Project prefix: paths include the project so consumers know which repo (#2054)
    const repoName = (repoRoot: string) => repoRoot === GATHERING_ROOT ? 'gathering' : 'chorus';

    const scanDir = (dir: string, repoRoot: string) => {
      if (!fs.existsSync(dir)) return;
      const entries = fs.readdirSync(dir, { recursive: true }) as string[];
      for (const entry of entries) {
        const entryStr = String(entry);
        if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
        const fullPath = path.join(dir, entryStr);
        try { if (!fs.statSync(fullPath).isFile()) continue; } catch { continue; }
        const relPath = path.relative(repoRoot, fullPath);
        const qualifiedPath = `${repoName(repoRoot)}/${relPath}`;
        const basename = path.basename(entryStr).toLowerCase();

        const relLower = relPath.toLowerCase();
        const pathParts = relLower.split('/');
        for (const [domainId, aliases] of Object.entries(aliasMap)) {
          for (const alias of aliases) {
            const nameMatch = basename.includes(alias) || basename.startsWith(alias + '.') || basename.startsWith(alias + '-');
            const pathMatch = pathParts.some(part => part === alias || part === alias + 's');
            if (nameMatch || pathMatch) {
              const ext = path.extname(entryStr).slice(1) || 'unknown';
              discovered.push({ domainId, filePath: qualifiedPath, fileType: ext });
              break;
            }
          }
        }
      }
    };

    scanDir(path.join(GATHERING_ROOT, 'src/handlers'), GATHERING_ROOT);
    scanDir(path.join(GATHERING_ROOT, 'src/services'), GATHERING_ROOT);
    scanDir(path.join(GATHERING_ROOT, 'src/adapters'), GATHERING_ROOT);
    scanDir(path.join(GATHERING_ROOT, 'tests'), GATHERING_ROOT);
    scanDir(path.join(CHORUS_ROOT, 'platform/scripts'), CHORUS_ROOT);
    scanDir(path.join(CHORUS_ROOT, 'platform/services/chorus-hooks/src'), CHORUS_ROOT);
    // Explicit directory→domain for code that doesn't name itself after its domain
    const dirDomainOverrides: Record<string, string> = {
      'platform/api/src': 'chorus-domain',
      'platform/api/tests': 'chorus-domain',
    };
    for (const [dir, domainId] of Object.entries(dirDomainOverrides)) {
      const fullDir = path.join(CHORUS_ROOT, dir);
      if (!fs.existsSync(fullDir)) continue;
      const entries = fs.readdirSync(fullDir, { recursive: true }) as string[];
      for (const entry of entries) {
        const entryStr = String(entry);
        if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
        const fullPath = path.join(fullDir, entryStr);
        try { if (!fs.statSync(fullPath).isFile()) continue; } catch { continue; }
        const relPath = path.relative(CHORUS_ROOT, fullPath);
        const qualifiedPath = `chorus/${relPath}`;
        const ext = path.extname(entryStr).slice(1) || 'unknown';
        discovered.push({ domainId, filePath: qualifiedPath, fileType: ext });
      }
    }
    scanDir(path.join(CHORUS_ROOT, 'skills'), CHORUS_ROOT);
    scanDir(path.join(CHORUS_ROOT, 'proving/domains/alerts'), CHORUS_ROOT);

    // 4. Clear existing code files and repopulate (idempotent)
    const clearQuery = `DELETE WHERE { GRAPH <urn:chorus:instances> { ?file a <https://jeffbridwell.com/chorus#CodeFile> ; ?p ?o . ?sd <https://jeffbridwell.com/chorus#hasCodeFile> ?file . } }`;
    await athenaSparqlUpdate(clearQuery);

    // 5. Write discovered files to graph in batches
    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < discovered.length; i += batchSize) {
      const batch = discovered.slice(i, i + batchSize);
      const triples = batch.map(d => {
        const fileId = `${d.domainId}-code-${d.filePath.replace(/[\/\.]/g, '-').toLowerCase()}`;
        const fileUri = `https://jeffbridwell.com/chorus#${fileId}`;
        const sdUri = `https://jeffbridwell.com/chorus#${d.domainId}`;
        return `<${fileUri}> a chorus:CodeFile ; rdfs:label "${d.filePath.replace(/"/g, '\\"')}" ; chorus:filePath "${d.filePath.replace(/"/g, '\\"')}" ; chorus:fileType "${d.fileType}" . <${sdUri}> chorus:hasCodeFile <${fileUri}> .`;
      }).join('\n');
      const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
      await athenaSparqlUpdate(insert);
      written += batch.length;
    }

    // 6. Summary by domain
    const byDomain: Record<string, number> = {};
    for (const d of discovered) {
      byDomain[d.domainId] = (byDomain[d.domainId] || 0) + 1;
    }

    res.json(athenaEnvelope('discover-code', {
      total_files: discovered.length,
      total_domains: Object.keys(byDomain).length,
      domains_available: domains.length,
      by_domain: byDomain,
      written,
    }, Date.now() - start, { count: discovered.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('discover-code', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/discover-tests — map test files to domains they cover (#1869)
app.post('/api/athena/discover-tests', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    // 1. Get all SubDomains for domain→alias mapping (reuse from discover-code)
    const sdQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }`;
    const sdResult = await athenaSparqlQuery(sdQuery);
    const domains = sdResult.results.bindings.map((b: any) => ({
      id: b.sd.value.split('#').pop() as string,
      label: (b.label.value as string).toLowerCase(),
    }));

    // Build reverse alias map: alias → domainId
    const genericBases = new Set(['services', 'service', 'domains', 'domain', 'code', 'loom', 'time', 'streams', 'stream', 'messages', 'message', 'policies', 'policy']);
    const aliasToId: Record<string, string> = {};
    for (const d of domains) {
      const base = d.id.replace(/-(domain|service)$/, '');
      if (genericBases.has(base)) continue;
      aliasToId[base] = d.id;
      if (base.endsWith('s') && !base.endsWith('ss')) {
        if (base.endsWith('ies')) aliasToId[base.replace(/ies$/, 'y')] = d.id;
        else aliasToId[base.replace(/s$/, '')] = d.id;
      }
    }
    // Special cases
    aliasToId['wordpress'] = 'blog-domain';
    aliasToId['socialpost'] = 'social-domain';
    aliasToId['sms-seed'] = 'seeds-domain';
    aliasToId['self-ai'] = 'sexuality-domain';
    aliasToId['ontology'] = 'convergence-domain';

    // 2. Scan test directories
    const GATHERING_ROOT = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site');
    const CHORUS_ROOT = path.resolve(__dirname, '../../..');
    const testEntries: Array<{ testFile: string; testType: string; coversDomain: string }> = [];

    const classifyTestType = (relPath: string): string => {
      if (/\/e2e\//i.test(relPath) || /\.e2e\./i.test(relPath)) return 'e2e';
      if (/\/integration\//i.test(relPath)) return 'integration';
      if (/\/performance\//i.test(relPath)) return 'performance';
      if (/\/security\//i.test(relPath)) return 'security';
      if (/\.bats$/i.test(relPath)) return 'bdd';
      if (/\.feature$/i.test(relPath)) return 'bdd';
      return 'unit';
    };

    const inferDomain = (filePath: string): string | null => {
      const basename = path.basename(filePath).toLowerCase();
      const pathLower = filePath.toLowerCase();
      for (const [alias, domainId] of Object.entries(aliasToId)) {
        if (basename.includes(alias) || pathLower.split('/').some(p => p === alias || p === alias + 's')) {
          return domainId;
        }
      }
      return null;
    };

    const scanTests = (dir: string, repoRoot: string) => {
      if (!fs.existsSync(dir)) return;
      const prefix = repoRoot === GATHERING_ROOT ? 'gathering' : 'chorus';
      const entries = fs.readdirSync(dir, { recursive: true }) as string[];
      for (const entry of entries) {
        const entryStr = String(entry);
        if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
        const fullPath = path.join(dir, entryStr);
        try { if (!fs.statSync(fullPath).isFile()) continue; } catch { continue; }
        if (!/\.(test|spec)\.(ts|js)$|\.bats$|\.feature$/i.test(entryStr)) continue;
        const relPath = path.relative(repoRoot, fullPath);
        const qualifiedPath = `${prefix}/${relPath}`;
        const testType = classifyTestType(relPath);
        const coversDomain = inferDomain(relPath);
        if (coversDomain) {
          testEntries.push({ testFile: qualifiedPath, testType, coversDomain });
        }
      }
    };

    scanTests(path.join(GATHERING_ROOT, 'tests'), GATHERING_ROOT);
    scanTests(path.join(CHORUS_ROOT, 'platform/api/tests'), CHORUS_ROOT);
    scanTests(path.join(CHORUS_ROOT, 'platform/services/chorus-hooks/tests'), CHORUS_ROOT);
    scanTests(path.join(CHORUS_ROOT, 'proving'), CHORUS_ROOT);
    scanTests(path.join(CHORUS_ROOT, 'docs/diagrams'), CHORUS_ROOT);

    // 3. Clear existing test coverage triples and repopulate
    const clearQuery = `DELETE WHERE { GRAPH <urn:chorus:instances> { ?t a <https://jeffbridwell.com/chorus#TestCoverage> ; ?p ?o . } }`;
    await athenaSparqlUpdate(clearQuery);

    // 4. Write to graph in batches
    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < testEntries.length; i += batchSize) {
      const batch = testEntries.slice(i, i + batchSize);
      const triples = batch.map(t => {
        const tcId = `test-coverage-${t.testFile.replace(/[\/\.]/g, '-').toLowerCase()}`;
        const tcUri = `https://jeffbridwell.com/chorus#${tcId}`;
        const sdUri = `https://jeffbridwell.com/chorus#${t.coversDomain}`;
        return `<${tcUri}> a chorus:TestCoverage ; chorus:testFile "${t.testFile.replace(/"/g, '\\"')}" ; chorus:testType "${t.testType}" ; chorus:covers <${sdUri}> .`;
      }).join('\n');
      const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
      await athenaSparqlUpdate(insert);
      written += batch.length;
    }

    // 5. Summary
    const byType: Record<string, number> = {};
    const byDomain: Record<string, number> = {};
    for (const t of testEntries) {
      byType[t.testType] = (byType[t.testType] || 0) + 1;
      byDomain[t.coversDomain] = (byDomain[t.coversDomain] || 0) + 1;
    }

    res.json(athenaEnvelope('discover-tests', {
      total_tests: testEntries.length,
      total_domains_covered: Object.keys(byDomain).length,
      by_type: byType,
      by_domain: byDomain,
      written,
    }, Date.now() - start, { count: testEntries.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('discover-tests', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/coverage — all test coverage for a domain (#1869)
app.get('/api/athena/subdomains/:id/coverage', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainCoverage({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/test-coverage — what tests cover this domain? (#1869)
app.get('/api/athena/subdomains/:id/test-coverage', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainTestCoverage({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// GET /api/chorus/tests/:domain + /api/chorus/tests — proxies to Gathering quality scanner (#2098, extracted #2189)
import { fetchTestsByDomain, fetchTestsAll } from './handlers/chorus-tests';
app.get('/api/chorus/tests/:domain', async (req: Request, res: Response) => {
  const r = await fetchTestsByDomain(req.params.domain, { envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});
app.get('/api/chorus/tests', async (_req: Request, res: Response) => {
  const r = await fetchTestsAll({ envelope: athenaEnvelope });
  res.status(r.status).json(r.body);
});

// POST /api/athena/discover-pages — auto-discover UI pages per domain from filesystem (#2065)
app.post('/api/athena/discover-pages', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    // 1. Get all SubDomains for domain→alias mapping (reuse pattern from discover-code)
    const sdQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }`;
    const sdResult = await athenaSparqlQuery(sdQuery);
    const domains = sdResult.results.bindings.map((b: any) => ({
      id: b.sd.value.split('#').pop() as string,
      label: (b.label.value as string).toLowerCase(),
    }));

    const genericBases = new Set(['services', 'service', 'domains', 'domain', 'code', 'loom', 'time', 'streams', 'stream', 'messages', 'message', 'policies', 'policy']);
    const aliasToId: Record<string, string> = {};
    for (const d of domains) {
      const base = d.id.replace(/-(domain|service)$/, '');
      if (genericBases.has(base)) continue;
      aliasToId[base] = d.id;
      if (base.endsWith('s') && !base.endsWith('ss')) {
        if (base.endsWith('ies')) aliasToId[base.replace(/ies$/, 'y')] = d.id;
        else aliasToId[base.replace(/s$/, '')] = d.id;
      }
    }
    aliasToId['blog'] = 'blog-domain';
    aliasToId['wordpress'] = 'blog-domain';
    aliasToId['social'] = 'social-domain';
    aliasToId['socialpost'] = 'social-domain';
    aliasToId['seed'] = 'seeds-domain';
    aliasToId['seeds'] = 'seeds-domain';
    aliasToId['self-ai'] = 'sexuality-domain';
    aliasToId['ontology'] = 'convergence-domain';
    aliasToId['chorus'] = 'chorus-domain';
    aliasToId['werk'] = 'chorus-domain';
    aliasToId['flow'] = 'chorus-domain';
    aliasToId['garden'] = 'property-domain';
    aliasToId['gardening'] = 'property-domain';

    const GATHERING_ROOT = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site');
    const entries: Array<{ route: string; path: string; pageType: string; domainId: string }> = [];

    // 2. Scan EJS views — classify by naming convention
    const viewsDir = path.join(GATHERING_ROOT, 'views');
    if (fs.existsSync(viewsDir)) {
      const viewFiles = fs.readdirSync(viewsDir).filter(f => f.endsWith('.ejs'));
      for (const file of viewFiles) {
        const name = file.replace('.ejs', '');
        let pageType = 'page';
        let domainId: string | null = null;

        // collection-{domain}.ejs
        const collectionMatch = name.match(/^collection-(.+?)(-list)?$/);
        if (collectionMatch) {
          pageType = 'collection';
          const alias = collectionMatch[1];
          domainId = aliasToId[alias] || null;
        }
        // {domain}-detail.ejs or {domain}-album.ejs
        const detailMatch = name.match(/^(.+?)-(detail|album|artist|artists|create)$/);
        if (!domainId && detailMatch) {
          pageType = 'detail';
          domainId = aliasToId[detailMatch[1]] || null;
        }
        // admin-{domain}-add.ejs or admin-harvest-{domain}.ejs
        const adminMatch = name.match(/^admin-(?:harvest-)?(.+?)(?:-add)?$/);
        if (!domainId && adminMatch) {
          pageType = 'admin';
          domainId = aliasToId[adminMatch[1]] || null;
        }
        // Direct domain name match or prefix match (e.g., seed-pipeline → seeds-domain)
        if (!domainId) {
          domainId = aliasToId[name] || null;
          if (!domainId) {
            for (const [alias, did] of Object.entries(aliasToId)) {
              if (name.startsWith(alias + '-') || name === alias) {
                domainId = did;
                break;
              }
            }
          }
        }

        if (domainId) {
          const route = pageType === 'collection' ? `/${collectionMatch![1]}` :
                        pageType === 'detail' ? `/${detailMatch![1]}/:slug` :
                        pageType === 'admin' ? `/admin/${name.replace('admin-', '')}` :
                        `/${name}`;
          entries.push({ route, path: `gathering/views/${file}`, pageType, domainId });
        }
      }

      // Ontology views
      const ontologyDir = path.join(viewsDir, 'ontology-views');
      if (fs.existsSync(ontologyDir)) {
        const ontologyFiles = fs.readdirSync(ontologyDir).filter(f => f.endsWith('.ejs'));
        for (const file of ontologyFiles) {
          const name = file.replace('.ejs', '');
          const domainId = aliasToId[name] || null;
          if (domainId) {
            entries.push({ route: `/ontology-views/${name}`, path: `gathering/views/ontology-views/${file}`, pageType: 'ontology', domainId });
          }
        }
      }
    }

    // 3. Scan static HTML — gathering-docs/domain-*.html and service designs
    const docsDir = path.join(GATHERING_ROOT, 'public/gathering-docs');
    if (fs.existsSync(docsDir)) {
      const htmlFiles = fs.readdirSync(docsDir).filter(f => f.endsWith('.html'));
      for (const file of htmlFiles) {
        const name = file.replace('.html', '');
        let domainId: string | null = null;
        let pageType = 'doc';

        // domain-{domain}.html
        const domainMatch = name.match(/^domain-(.+)$/);
        if (domainMatch) {
          domainId = aliasToId[domainMatch[1]] || null;
          pageType = 'doc';
        }
        // {domain}-service-design.html
        const serviceMatch = name.match(/^(.+?)-service-design$/);
        if (!domainId && serviceMatch) {
          domainId = aliasToId[serviceMatch[1]] || null;
          pageType = 'service-design';
        }

        if (domainId) {
          entries.push({ route: `/gathering-docs/${file}`, path: `gathering/public/gathering-docs/${file}`, pageType, domainId });
        }
      }
    }

    // 4. Clear existing page data and repopulate
    const clearQuery = `DELETE WHERE { GRAPH <urn:chorus:instances> { ?p a <https://jeffbridwell.com/chorus#Page> ; ?prop ?val . ?sd <https://jeffbridwell.com/chorus#hasPage> ?p . } }`;
    await athenaSparqlUpdate(clearQuery);

    // 5. Write to graph in batches
    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < entries.length; i += batchSize) {
      const batch = entries.slice(i, i + batchSize);
      const triples = batch.map(e => {
        const pageId = `page-${e.path.replace(/[\/\.]/g, '-').toLowerCase()}`;
        const pageUri = `https://jeffbridwell.com/chorus#${pageId}`;
        const sdUri = `https://jeffbridwell.com/chorus#${e.domainId}`;
        return `<${pageUri}> a chorus:Page ; rdfs:label "${e.route.replace(/"/g, '\\"')}" ; chorus:filePath "${e.path.replace(/"/g, '\\"')}" ; chorus:pageType "${e.pageType}" ; chorus:route "${e.route.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPage <${pageUri}> .`;
      }).join('\n');
      const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
      await athenaSparqlUpdate(insert);
      written += batch.length;
    }

    // 6. Summary by domain
    const byDomain: Record<string, number> = {};
    for (const e of entries) { byDomain[e.domainId] = (byDomain[e.domainId] || 0) + 1; }

    res.json(athenaEnvelope('discover-pages', {
      total_pages: entries.length,
      total_domains: Object.keys(byDomain).length,
      by_domain: byDomain,
      entries,
      written,
    }, Date.now() - start, { count: entries.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('discover-pages', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/pages — pages for a domain (#2065)
app.get('/api/athena/subdomains/:id/pages', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainPages({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/discover-endpoints — auto-discover API endpoints per domain (#2066)
app.post('/api/athena/discover-endpoints', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    // 1. Domain alias map (same as discover-code/discover-pages)
    const sdQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }`;
    const sdResult = await athenaSparqlQuery(sdQuery);
    const domains = sdResult.results.bindings.map((b: any) => ({
      id: b.sd.value.split('#').pop() as string,
      label: (b.label.value as string).toLowerCase(),
    }));

    // Handler variable name → domain mapping
    const handlerToDomain: Record<string, string> = {};
    for (const d of domains) {
      const base = d.id.replace(/-(domain|service)$/, '');
      handlerToDomain[base + 'Handler'] = d.id;
      // Singular forms
      if (base.endsWith('s') && !base.endsWith('ss')) {
        const singular = base.endsWith('ies') ? base.replace(/ies$/, 'y') : base.replace(/s$/, '');
        handlerToDomain[singular + 'Handler'] = d.id;
      }
    }
    // Explicit overrides for non-obvious handler names
    handlerToDomain['bookHandler'] = 'books-domain';
    handlerToDomain['bookUploadHandler'] = 'books-domain';
    handlerToDomain['seedHandler'] = 'seeds-domain';
    handlerToDomain['socialpostHandler'] = 'social-domain';
    handlerToDomain['personHandler'] = 'people-domain';
    handlerToDomain['collectionHandler'] = 'blog-domain';
    handlerToDomain['glimmerHandler'] = 'glimmers-domain';
    handlerToDomain['ideaProjectHandler'] = 'ideas-domain';
    handlerToDomain['codebaseGraphHandler'] = 'chorus-domain';
    handlerToDomain['dashboardHandler'] = 'chorus-domain';
    handlerToDomain['flowHandler'] = 'chorus-domain';
    handlerToDomain['werkHandler'] = 'chorus-domain';
    handlerToDomain['ontologyViewHandler'] = 'convergence-domain';
    handlerToDomain['galleryHandler'] = 'gallery-domain';
    handlerToDomain['gardenHandler'] = 'property-domain';
    handlerToDomain['icdHandler'] = 'convergence-domain';
    handlerToDomain['docCatalogHandler'] = 'documents-domain';
    handlerToDomain['docsHandler'] = 'documents-domain';
    handlerToDomain['documentHandler'] = 'documents-domain';
    handlerToDomain['accessDashboardHandler'] = 'chorus-domain';
    handlerToDomain['aclHandler'] = 'chorus-domain';
    handlerToDomain['sessionReplayHandler'] = 'chorus-domain';
    handlerToDomain['staticPageHandler'] = 'chorus-domain';
    handlerToDomain['linkInferenceHandler'] = 'knowledge-domain';
    handlerToDomain['knowledgeGraphHandler'] = 'knowledge-domain';
    handlerToDomain['selfDomainHandler'] = 'self-domain';
    handlerToDomain['selfAiHandler'] = 'sexuality-domain';
    handlerToDomain['sexualityHandler'] = 'sexuality-domain';
    // Silas review — missing handlers added
    handlerToDomain['cookingHandler'] = 'cooking-domain';
    handlerToDomain['fitnessFunctionsHandler'] = 'chorus-domain';
    handlerToDomain['intentionHandler'] = 'ideas-domain';
    handlerToDomain['notesHandler'] = 'notes-domain';
    handlerToDomain['noteHandler'] = 'notes-domain';
    handlerToDomain['readingHandler'] = 'reading-domain';
    handlerToDomain['storiesHandler'] = 'stories-domain';
    handlerToDomain['storyHandler'] = 'stories-domain';
    handlerToDomain['watchingHandler'] = 'watching-domain';
    handlerToDomain['todoHandler'] = 'ideas-domain';
    handlerToDomain['groupHandler'] = 'people-domain';
    handlerToDomain['qualityHandler'] = 'chorus-domain';
    handlerToDomain['rolesHandler'] = 'roles-domain';
    handlerToDomain['skillsHandler'] = 'skills-service';
    handlerToDomain['teamHandler'] = 'chorus-domain';
    handlerToDomain['briefsHandler'] = 'chorus-domain';
    handlerToDomain['cardsHandler'] = 'cards-service';
    handlerToDomain['costHandler'] = 'chorus-domain';
    handlerToDomain['hooksHandler'] = 'chorus-domain';
    handlerToDomain['decisionsHandler'] = 'chorus-domain';
    handlerToDomain['gardeningHandler'] = 'property-domain';
    handlerToDomain['webhookHandler'] = 'seeds-domain';
    handlerToDomain['userHandler'] = 'chorus-domain';
    handlerToDomain['aboutHandler'] = 'chorus-domain';
    handlerToDomain['aboutProfileHandler'] = 'chorus-domain';
    handlerToDomain['homeHandler'] = 'chorus-domain';
    handlerToDomain['loginHandler'] = 'chorus-domain';
    handlerToDomain['callbackHandler'] = 'chorus-domain';
    handlerToDomain['profileHandler'] = 'chorus-domain';
    handlerToDomain['logoutHandler'] = 'chorus-domain';

    // Route prefix → domain (fallback for routes without handler reference)
    const routePrefixToDomain: Record<string, string> = {
      '/api/books': 'books-domain', '/books': 'books-domain',
      '/api/music': 'music-domain', '/music': 'music-domain',
      '/api/photos': 'photos-domain', '/photos': 'photos-domain',
      '/api/property': 'property-domain', '/property': 'property-domain',
      '/api/seed': 'seeds-domain',
      '/api/glimmers': 'glimmers-domain',
      '/api/ideas': 'ideas-domain',
      '/api/collections': 'blog-domain', '/blog': 'blog-domain',
      '/api/search': 'search-domain', '/search': 'search-domain',
      '/api/gallery': 'gallery-domain', '/gallery': 'gallery-domain',
      '/api/documents': 'documents-domain', '/documents': 'documents-domain',
      '/api/codebase': 'chorus-domain',
      '/api/dashboard': 'chorus-domain', '/dashboard': 'chorus-domain',
      '/api/admin': 'chorus-domain',
      '/api/icd': 'convergence-domain',
      '/api/chorus': 'chorus-domain',
      '/api/athena': 'chorus-domain',
      '/cooking': 'cooking-domain',
      '/notes': 'notes-domain',
      '/reading': 'reading-domain',
      '/stories': 'stories-domain',
      '/watching': 'watching-domain',
      '/todo': 'ideas-domain',
      '/gardening': 'property-domain',
      '/people': 'people-domain',
      '/socialposts': 'social-domain',
      '/self': 'self-domain',
      '/sexuality': 'sexuality-domain',
      '/api/sessions': 'chorus-domain',
      '/api/roles': 'roles-domain',
    };

    const GATHERING_ROOT = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site');
    const entries: Array<{ method: string; path: string; handler: string; domainId: string }> = [];

    // 2. Parse app.ts for route definitions
    const appTsPath = path.join(GATHERING_ROOT, 'src/app.ts');
    if (fs.existsSync(appTsPath)) {
      const appContent = fs.readFileSync(appTsPath, 'utf-8');
      // Match: app.get('/path', ..., handlerName.methodName) or app.get('/path', (req, res) => ...)
      const routeRegex = /app\.(get|post|put|delete|patch)\s*\(\s*['"`]([^'"`]+)['"`]/gi;
      let match;
      while ((match = routeRegex.exec(appContent)) !== null) {
        const method = match[1].toUpperCase();
        const routePath = match[2];
        // Find handler reference on the same or next few chars
        const lineEnd = appContent.indexOf('\n', match.index);
        const lineContent = appContent.substring(match.index, lineEnd > 0 ? lineEnd : match.index + 200);
        // Extract handler variable name (e.g., bookHandler.renderCollection)
        const handlerMatch = lineContent.match(/(\w+Handler)\.\w+/);
        const handlerName = handlerMatch ? handlerMatch[1] : null;

        // Map to domain
        let domainId: string | null = null;
        if (handlerName && handlerToDomain[handlerName]) {
          domainId = handlerToDomain[handlerName];
        }
        if (!domainId) {
          // Try route prefix matching
          for (const [prefix, did] of Object.entries(routePrefixToDomain)) {
            if (routePath.startsWith(prefix)) {
              domainId = did;
              break;
            }
          }
        }

        if (domainId) {
          entries.push({
            method,
            path: routePath,
            handler: handlerName ? `gathering/src/handlers/${handlerName.replace(/Handler$/, '')}.handler.ts` : 'gathering/src/app.ts',
            domainId,
          });
        }
      }
    }

    // 3. Clear existing endpoint data and repopulate
    const clearQuery = `DELETE WHERE { GRAPH <urn:chorus:instances> { ?ep a <https://jeffbridwell.com/chorus#Endpoint> ; ?p ?o . ?sd <https://jeffbridwell.com/chorus#hasEndpoint> ?ep . } }`;
    await athenaSparqlUpdate(clearQuery);

    // 4. Write to graph in batches
    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < entries.length; i += batchSize) {
      const batch = entries.slice(i, i + batchSize);
      const triples = batch.map(e => {
        const epId = `endpoint-${e.method.toLowerCase()}-${e.path.replace(/[\/\.:]/g, '-').toLowerCase()}`;
        const epUri = `https://jeffbridwell.com/chorus#${epId}`;
        const sdUri = `https://jeffbridwell.com/chorus#${e.domainId}`;
        return `<${epUri}> a chorus:Endpoint ; rdfs:label "${e.method} ${e.path.replace(/"/g, '\\"')}" ; chorus:httpMethod "${e.method}" ; chorus:routePath "${e.path.replace(/"/g, '\\"')}" ; chorus:filePath "${e.handler.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasEndpoint <${epUri}> .`;
      }).join('\n');
      const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
      await athenaSparqlUpdate(insert);
      written += batch.length;
    }

    // 5. Summary
    const byDomain: Record<string, number> = {};
    for (const e of entries) { byDomain[e.domainId] = (byDomain[e.domainId] || 0) + 1; }

    res.json(athenaEnvelope('discover-endpoints', {
      total_endpoints: entries.length,
      total_domains: Object.keys(byDomain).length,
      by_domain: byDomain,
      entries,
      written,
    }, Date.now() - start, { count: entries.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('discover-endpoints', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/services — API endpoints for a domain (#2066)
app.get('/api/athena/subdomains/:id/services', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainEndpoints({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/actors — actors that interact with this subdomain (#1899)
app.get('/api/athena/subdomains/:id/actors', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainActors({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/scenarios — BDD scenarios for this subdomain (#1899)
app.get('/api/athena/subdomains/:id/scenarios', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainScenarios({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/contract — API contract for this subdomain (#1899)
app.get('/api/athena/subdomains/:id/contract', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainContract({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/chorus/open — open a file locally (#1907)
app.options('/api/chorus/open', (_req: Request, res: Response) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'POST, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type');
  res.sendStatus(204);
});
app.post('/api/chorus/open', (req: Request, res: Response) => {
  res.header('Access-Control-Allow-Origin', '*');
  const { path: filePath } = req.body || {};
  if (!filePath) return res.status(400).json({ error: 'Missing path' });
  const resolved = path.resolve(REPO_ROOT, filePath);
  if (!resolved.startsWith(REPO_ROOT)) return res.status(403).json({ error: 'Path outside repo' });
  const { execSync } = require('child_process');
  try {
    execSync(`open "${resolved}"`);
    res.json({ ok: true, opened: resolved });
  } catch (err: any) {
    res.status(500).json({ error: err.message });
  }
});

// NOTE: duplicate GET /api/athena/subdomains/:id/pages removed (#2187).
// Express matches routes in registration order; the earlier definition at
// src/handlers/athena-subdomain-pages.ts handled every request for this path.
// The second copy (different response shape) was unreachable dead code.

// POST /api/athena/subdomains/:id/pages — add page to subdomain (#1923)
app.post('/api/athena/subdomains/:id/pages', async (req: Request, res: Response) => {
  const r = await createSubdomainPage(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/integrations — data integrations for this subdomain (#1923)
app.get('/api/athena/subdomains/:id/integrations', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainIntegrations({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/integrations — add integration to subdomain (#1923)
app.post('/api/athena/subdomains/:id/integrations', async (req: Request, res: Response) => {
  const r = await createSubdomainIntegration(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/persistence — persistence stores for this subdomain (#1923)
app.get('/api/athena/subdomains/:id/persistence', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainPersistence({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

app.post('/api/athena/subdomains/:id/persistence', async (req: Request, res: Response) => {
  const r = await createSubdomainPersistence(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/services — runtime services for this subdomain (#1924)
// Extracted to handlers/subdomain-entities.ts (#2180). Four list-GETs share
// fetchSubdomainEntities + spec — same subdomainExists check, same shape.
import {
  fetchSubdomainServicesList,
  fetchSubdomainPipelineList,
  fetchSubdomainLogsList,
  fetchSubdomainGapsList,
  createSubdomainService,
  createSubdomainPipeline,
  createSubdomainLog,
  createSubdomainGap,
  createSubdomainPage,
  createSubdomainIntegration,
  createSubdomainPersistence,
  createSubdomainScenario,
  createSubdomainActor,
  createSubdomainContract,
  createSubdomainPriorArt,
  updateSubdomainActor,
  updateSubdomainScenario,
  updateSubdomainContract,
  updateSubdomainPriorArt,
  updateSubdomainService,
  updateSubdomainPipeline,
  updateSubdomainLog,
  updateSubdomainGap,
  updateSubdomainPage,
  updateSubdomainIntegration,
  updateSubdomainPersistence,
  deleteSubdomainEntity,
} from './handlers/subdomain-entities';

const subdomainWriteDeps = () => ({
  ...domainFacetDeps(),
  sparqlUpdate: athenaSparqlUpdate,
});

app.get('/api/athena/subdomains/:id/services', async (req: Request, res: Response) => {
  const r = await fetchSubdomainServicesList(domainFacetDeps(), req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/services — add service to subdomain (#1924)
app.post('/api/athena/subdomains/:id/services', async (req: Request, res: Response) => {
  const r = await createSubdomainService(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/pipeline — data pipeline for this subdomain (#1925)
app.get('/api/athena/subdomains/:id/pipeline', async (req: Request, res: Response) => {
  const r = await fetchSubdomainPipelineList(domainFacetDeps(), req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/pipeline — add pipeline to subdomain (#1925)
app.post('/api/athena/subdomains/:id/pipeline', async (req: Request, res: Response) => {
  const r = await createSubdomainPipeline(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/logs — log sources for this subdomain (#1926)
app.get('/api/athena/subdomains/:id/logs', async (req: Request, res: Response) => {
  const r = await fetchSubdomainLogsList(domainFacetDeps(), req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/logs — add log source to subdomain (#1926)
app.post('/api/athena/subdomains/:id/logs', async (req: Request, res: Response) => {
  const r = await createSubdomainLog(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/gaps — known gaps for this subdomain (#1926)
app.get('/api/athena/subdomains/:id/gaps', async (req: Request, res: Response) => {
  const r = await fetchSubdomainGapsList(domainFacetDeps(), req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/gaps — add gap to subdomain (#1926)
app.post('/api/athena/subdomains/:id/gaps', async (req: Request, res: Response) => {
  const r = await createSubdomainGap(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/prior-art — prior art for this subdomain (#1907)
app.get('/api/athena/subdomains/:id/prior-art', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainPriorArt({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/prior-art — add prior art to subdomain (#1907)
app.post('/api/athena/subdomains/:id/prior-art', async (req: Request, res: Response) => {
  const r = await createSubdomainPriorArt(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/actors — add actor to subdomain (#1899)
app.post('/api/athena/subdomains/:id/actors', async (req: Request, res: Response) => {
  const r = await createSubdomainActor(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// DELETE /api/athena/subdomains/:id/:section/:entityId — extracted to
// handlers/subdomain-entities.ts::deleteSubdomainEntity (#2180). The
// section→class/predicate table (ENTITY_SECTIONS) now lives in the
// handler module too.
app.delete('/api/athena/subdomains/:id/:section/:entityId', async (req: Request, res: Response) => {
  const r = await deleteSubdomainEntity(subdomainWriteDeps(), req.params.id, req.params.section, req.params.entityId);
  if (r.status === 204) { res.status(204).send(); return; }
  res.status(r.status).json(r.body);
});

app.put('/api/athena/subdomains/:id/actors/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainActor(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

app.put('/api/athena/subdomains/:id/scenarios/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainScenario(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

app.put('/api/athena/subdomains/:id/contract/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainContract(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

// PUT /api/athena/subdomains/:id/pages/:entityId (#1929)
// PUT adapters — 7 handlers, each 3 lines (#2180).
app.put('/api/athena/subdomains/:id/pages/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainPage(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});
app.put('/api/athena/subdomains/:id/integrations/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainIntegration(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});
app.put('/api/athena/subdomains/:id/persistence/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainPersistence(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});
app.put('/api/athena/subdomains/:id/services/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainService(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});
app.put('/api/athena/subdomains/:id/pipeline/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainPipeline(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

// PUT /api/athena/subdomains/:id/logs/:entityId (#1929)
app.put('/api/athena/subdomains/:id/logs/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainLog(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});
app.put('/api/athena/subdomains/:id/gaps/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainGap(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

app.put('/api/athena/subdomains/:id/prior-art/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainPriorArt(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains/:id/scenarios — add BDD scenario to subdomain (#1899)
app.post('/api/athena/subdomains/:id/scenarios', async (req: Request, res: Response) => {
  const r = await createSubdomainScenario(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

app.post('/api/athena/subdomains/:id/contract', async (req: Request, res: Response) => {
  const r = await createSubdomainContract(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/completeness — lifecycle-gated completeness score (#1899, #1979)
// #1979: Split into 2 parallel queries — metadata (ontology) + instance counts (instances).
// The original monolithic query had 11 OPTIONAL cross-graph joins that caused
// Fuseki timeout on populated domains due to combinatorial explosion.
app.get('/api/athena/subdomains/:id/completeness', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainCompleteness(
    { sparqlQuery: athenaSparqlQuery, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// POST /api/athena/subdomains — create a new SubDomain
app.post('/api/athena/subdomains', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { id, label, owner, step, comment } = req.body || {};
    if (!id || !label) {
      return res.status(400).json(athenaEnvelope('subdomain-create', {
        error: 'Missing required fields: id, label',
        example: { id: 'my-domain', label: 'My Domain', owner: 'Wren', step: 'Building', comment: 'Description' },
      }, Date.now() - start, { error: true }));
    }
    const uri = `https://jeffbridwell.com/chorus#${id}`;
    const ownerMap: Record<string, string> = { wren: 'chorus:wren', silas: 'chorus:silas', kade: 'chorus:kade', jeff: 'chorus:jeff' };
    const stepMap: Record<string, string> = {
      capturing: 'chorus:capturing', shaping: 'chorus:shaping', designing: 'chorus:designing',
      building: 'chorus:building', proving: 'chorus:proving', directing: 'chorus:directing',
    };
    let triples = `<${uri}> a chorus:SubDomain ; rdfs:label "${label}"`;
    if (owner && ownerMap[owner.toLowerCase()]) triples += ` ; chorus:ownedBy ${ownerMap[owner.toLowerCase()]}`;
    if (step && stepMap[step.toLowerCase()]) triples += ` ; chorus:primaryStep ${stepMap[step.toLowerCase()]}`;
    if (comment) triples += ` ; rdfs:comment "${comment.replace(/"/g, '\\"')}"`;
    triples += ' .';
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#>\nPREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nINSERT DATA { GRAPH <${ATHENA_INSTANCES}> { ${triples} } }`;
    await athenaSparqlUpdate(update);
    res.status(201).json(athenaEnvelope('subdomain-create', { uri, id, label, owner: owner || null, step: step || null, comment: comment || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// PUT /api/athena/subdomains/:id — update SubDomain properties
app.put('/api/athena/subdomains/:id', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const uri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const { label, owner, step, comment } = req.body || {};
    if (!label && !owner && !step && !comment) {
      return res.status(400).json(athenaEnvelope('subdomain-update', {
        error: 'No fields to update. Provide at least one of: label, owner, step, comment',
      }, Date.now() - start, { error: true }));
    }
    const ownerMap: Record<string, string> = { wren: 'chorus:wren', silas: 'chorus:silas', kade: 'chorus:kade', jeff: 'chorus:jeff' };
    const stepMap: Record<string, string> = {
      capturing: 'chorus:capturing', shaping: 'chorus:shaping', designing: 'chorus:designing',
      building: 'chorus:building', proving: 'chorus:proving', directing: 'chorus:directing',
    };
    const deletes: string[] = [];
    const inserts: string[] = [];
    if (label) { deletes.push(`<${uri}> rdfs:label ?oldLabel .`); inserts.push(`<${uri}> rdfs:label "${label}" .`); }
    if (owner && ownerMap[owner.toLowerCase()]) { deletes.push(`<${uri}> chorus:ownedBy ?oldOwner .`); inserts.push(`<${uri}> chorus:ownedBy ${ownerMap[owner.toLowerCase()]} .`); }
    if (step && stepMap[step.toLowerCase()]) { deletes.push(`<${uri}> chorus:primaryStep ?oldStep .`); inserts.push(`<${uri}> chorus:primaryStep ${stepMap[step.toLowerCase()]} .`); }
    if (comment) { deletes.push(`<${uri}> rdfs:comment ?oldComment .`); inserts.push(`<${uri}> rdfs:comment "${comment.replace(/"/g, '\\"')}" .`); }
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#>\nPREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nWITH <${ATHENA_INSTANCES}>\nDELETE { ${deletes.join(' ')} }\nINSERT { ${inserts.join(' ')} }\nWHERE { <${uri}> a chorus:SubDomain . ${deletes.map(d => `OPTIONAL { ${d} }`).join(' ')} }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-update', { uri, id: req.params.id, updated: { label, owner, step, comment } }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-update', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/consumes — add consumption edge
app.post('/api/athena/subdomains/:id/consumes', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { targetId } = req.body || {};
    if (!targetId) {
      return res.status(400).json(athenaEnvelope('subdomain-consumes-add', {
        error: 'Missing required field: targetId',
        example: { targetId: 'security-domain' },
      }, Date.now() - start, { error: true }));
    }
    const sourceUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const targetUri = `https://jeffbridwell.com/chorus#${targetId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#>\nINSERT DATA { GRAPH <${ATHENA_INSTANCES}> { <${sourceUri}> chorus:consumes <${targetUri}> . } }`;
    await athenaSparqlUpdate(update);
    res.status(201).json(athenaEnvelope('subdomain-consumes-add', { source: req.params.id, target: targetId }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-consumes-add', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// DELETE /api/athena/subdomains/:id/consumes/:targetId — remove consumption edge
app.delete('/api/athena/subdomains/:id/consumes/:targetId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sourceUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const targetUri = `https://jeffbridwell.com/chorus#${req.params.targetId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#>\nDELETE DATA { GRAPH <${ATHENA_INSTANCES}> { <${sourceUri}> chorus:consumes <${targetUri}> . } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-consumes-remove', { source: req.params.id, target: req.params.targetId }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-consumes-remove', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/reload — reload ontology from TTL into Fuseki
app.post('/api/athena/reload', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const ttlPath = path.join(REPO_ROOT, 'roles/silas/ontology/chorus.ttl');
    if (!fs.existsSync(ttlPath)) {
      return res.status(404).json(athenaEnvelope('reload', { error: `TTL file not found: ${ttlPath}` }, Date.now() - start, { error: true }));
    }
    const ttlContent = fs.readFileSync(ttlPath, 'utf-8');
    // #1956: Only drop+replace ontology graph. Instances graph (API-created data) is untouched.
    await athenaSparqlUpdate(`DROP SILENT GRAPH <${ATHENA_GRAPH}>`);
    const loadRes = await fetch('http://localhost:3030/pods/data?graph=' + encodeURIComponent(ATHENA_GRAPH), {
      method: 'PUT',
      headers: { 'Content-Type': 'text/turtle' },
      body: ttlContent,
    });
    if (!loadRes.ok) {
      const text = await loadRes.text();
      throw new Error(`Fuseki load ${loadRes.status}: ${text.slice(0, 200)}`);
    }
    const countResult = await athenaSparqlQuery(loadSparql('health'));
    const tripleCount = parseInt(countResult.results.bindings[0]?.count?.value || '0', 10);
    res.json(athenaEnvelope('reload', { status: 'ok', source: ttlPath, tripleCount }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('reload', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/validate — consumer declares predicate dependencies, API checks if they exist (#1356)
app.post('/api/athena/validate', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const predicates: string[] = req.body?.predicates;
    if (!Array.isArray(predicates) || predicates.length === 0) {
      return res.status(400).json(athenaEnvelope('validate', { error: 'Body must include predicates: string[]' }, Date.now() - start, { error: true }));
    }

    const prefixMap: Record<string, string> = {
      'chorus:': 'https://jeffbridwell.com/chorus#',
      'rdfs:': 'http://www.w3.org/2000/01/rdf-schema#',
      'owl:': 'http://www.w3.org/2002/07/owl#',
      'rdf:': 'http://www.w3.org/1999/02/22-rdf-syntax-ns#',
    };

    const valid: string[] = [];
    const missing: string[] = [];

    for (const pred of predicates) {
      let fullUri = pred;
      for (const [prefix, uri] of Object.entries(prefixMap)) {
        if (pred.startsWith(prefix)) {
          fullUri = pred.replace(prefix, uri);
          break;
        }
      }
      const query = `ASK WHERE { GRAPH <${ATHENA_GRAPH}> { ?s <${fullUri}> ?o } }`;
      try {
        const result = await athenaSparqlQuery(query);
        if (result.boolean) {
          valid.push(pred);
        } else {
          missing.push(pred);
        }
      } catch {
        missing.push(pred);
      }
    }

    res.json(athenaEnvelope('validate', { valid, missing, total: predicates.length, valid_count: valid.length, missing_count: missing.length }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('validate', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/card/:id — card detail for inline rendering (#1900)
app.get('/api/athena/card/:id', async (req: Request, res: Response) => {
  const cardsScript = path.resolve(__dirname, '../../scripts/cards');
  const env = { ...process.env, PATH: `/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` };
  const r = await fetchAthenaCardDetail(
    {
      runCardsView: async (cardId: string) => {
        const { stdout } = await execAsync(`bash ${cardsScript} view ${cardId} --json 2>/dev/null`, { encoding: 'utf-8', timeout: 10000, env });
        return stdout;
      },
      envelope: athenaEnvelope,
    },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// 404 handler for unknown /api/athena/* paths — agent-friendly suggestions
app.use('/api/athena', (_req: Request, res: Response) => {
  res.status(404).json(athenaEnvelope('unknown', {
    error: `Unknown Athena endpoint: ${_req.path}`,
    suggestion: 'Use GET /api/athena/health to discover available endpoints.',
    available: ATHENA_QUERIES.map(q => q.path),
  }, 0, { error: true }));
});

// --- RCA (Root Cause Analysis) domain — #1795 ---

const RCA_DB_PATH = DB_PATH; // Same SQLite as chorus index

function ensureRcaTable(): void {
  const db = new Database(RCA_DB_PATH);
  db.pragma('journal_mode = WAL');
  db.exec(`CREATE TABLE IF NOT EXISTS rcas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    trigger_event TEXT NOT NULL,
    timeline TEXT,
    root_cause TEXT NOT NULL,
    contributing_factors TEXT DEFAULT '[]',
    corrective_actions TEXT DEFAULT '[]',
    cards TEXT DEFAULT '[]',
    spine_events TEXT DEFAULT '[]',
    status TEXT DEFAULT 'open',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  )`);
  db.close();
}

// Lazy init on first use
let rcaTableReady = false;

app.post('/api/chorus/rca', (req: Request, res: Response) => {
  if (!rcaTableReady) { ensureRcaTable(); rcaTableReady = true; }

  const { title, trigger, timeline, root_cause, contributing_factors, corrective_actions, cards, spine_events } = req.body || {};

  if (!title || !trigger || !root_cause) {
    res.status(400).json({ error: 'title, trigger, and root_cause are required' });
    return;
  }

  const validStatuses = ['open', 'verified', 'closed'];
  const status = validStatuses.includes(req.body.status) ? req.body.status : 'open';
  const now = bostonNow();

  const db = new Database(RCA_DB_PATH);
  db.pragma('journal_mode = WAL');

  const result = db.prepare(`
    INSERT INTO rcas (title, trigger_event, timeline, root_cause, contributing_factors, corrective_actions, cards, spine_events, status, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    title,
    trigger,
    timeline || '',
    root_cause,
    JSON.stringify(contributing_factors || []),
    JSON.stringify(corrective_actions || []),
    JSON.stringify(cards || []),
    JSON.stringify(spine_events || []),
    status,
    now,
    now,
  );

  db.close();

  // Emit spine event
  const CHORUS_LOG = `${CHORUS_ROOT}/platform/logs/chorus.log`;
  const entry = JSON.stringify({
    timestamp: now,
    level: 'info',
    appName: 'chorus-events',
    component: 'rca',
    event: 'rca.created',
    role: 'system',
    rca_id: String(result.lastInsertRowid),
    cards: JSON.stringify(cards || []),
  });
  fs.appendFileSync(CHORUS_LOG, entry + '\n');

  res.json({ ok: true, id: result.lastInsertRowid, status });
});

import { fetchChorusRcas } from './handlers/chorus-rcas';
app.get('/api/chorus/rcas', (req: Request, res: Response) => {
  if (!rcaTableReady) { ensureRcaTable(); rcaTableReady = true; }
  const db = new Database(RCA_DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    const r = fetchChorusRcas({ db }, { status: req.query.status as string | undefined });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

// --- Spine Event Service — #2109 ---
// Decoupled spine endpoint: Gathering POSTs here instead of importing chorus-sdk.
// Events with hop fields auto-create trace entries.

app.post('/api/chorus/spine-event', (req: Request, res: Response) => {
  const { event, role, ...fields } = req.body || {};

  if (!event) {
    res.status(400).json({ error: 'event is required' });
    return;
  }

  // Write to chorus.log (same as chorus-sdk emit did in-process)
  const entry = {
    timestamp: bostonNow(),
    level: 'info',
    appName: 'chorus-events',
    component: 'spine-service',
    event,
    role: role || 'system',
    ...fields,
  };
  const CHORUS_LOG = `${CHORUS_ROOT}/chorus/platform/logs/chorus.log`;
  try {
    fs.appendFileSync(CHORUS_LOG, JSON.stringify(entry) + '\n');
  } catch { /* best effort */ }

  // Auto-create trace hop if hop field present
  if (typeof fields.hop === 'number' && !isNaN(fields.hop)) {
    if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; }
    const db = new Database(DB_PATH);
    db.pragma('journal_mode = WAL');
    const now = bostonNow();
    db.prepare(`
      INSERT INTO traces (correlation_id, hop, call_stack, source_domain, source_service, source_instance, dest_domain, dest_service, dest_instance, timestamp, latency_ms, error_class, error_message, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      fields.trace_id || `spine-${Date.now()}`,
      fields.hop,
      fields.callStack || 'integration',
      fields.domain || null,
      fields.source_service || event,
      fields.source_instance || null,
      fields.dest_domain || fields.domain || null,
      fields.dest_service || null,
      fields.dest_instance || null,
      now,
      fields.latencyMs || null,
      fields.error_class || null,
      fields.error_message || null,
      now,
    );
    db.close();
  }

  res.json({ ok: true });
});

// --- Trace Envelope — #2097 (ADR-024) ---
// Common message envelope with hop-level tracing across four call stacks.
// Traces auto-populate domain integration maps.

function ensureTraceTable(): void {
  const db = new Database(DB_PATH);
  db.pragma('journal_mode = WAL');
  db.exec(`CREATE TABLE IF NOT EXISTS traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    correlation_id TEXT NOT NULL,
    hop INTEGER NOT NULL,
    call_stack TEXT NOT NULL,
    source_domain TEXT,
    source_service TEXT,
    source_instance TEXT,
    dest_domain TEXT,
    dest_service TEXT,
    dest_instance TEXT,
    timestamp TEXT NOT NULL,
    latency_ms INTEGER,
    error_class TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL
  )`);
  // Indexes for fast lookup
  const hasIdx = db.prepare("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_traces_corr'").get();
  if (!hasIdx) {
    db.exec(`CREATE INDEX idx_traces_corr ON traces(correlation_id)`);
    db.exec(`CREATE INDEX idx_traces_domain ON traces(source_domain)`);
  }
  db.close();
}

let traceTableReady = false;

// POST /api/chorus/trace — record a hop
app.post('/api/chorus/trace', (req: Request, res: Response) => {
  if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; }

  const { correlationId, hop, callStack, source, destination, latencyMs, error } = req.body || {};

  if (!correlationId || !hop || !callStack) {
    res.status(400).json({ error: 'correlationId, hop, and callStack are required' });
    return;
  }

  const now = bostonNow();
  const db = new Database(DB_PATH);
  db.pragma('journal_mode = WAL');

  db.prepare(`
    INSERT INTO traces (correlation_id, hop, call_stack, source_domain, source_service, source_instance, dest_domain, dest_service, dest_instance, timestamp, latency_ms, error_class, error_message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    correlationId,
    hop,
    callStack,
    source?.domain || null,
    source?.service || null,
    source?.instance || null,
    destination?.domain || null,
    destination?.service || null,
    destination?.instance || null,
    now,
    latencyMs || null,
    error?.classification || null,
    error?.message || null,
    now,
  );

  db.close();
  res.json({ ok: true });
});

// /api/chorus/trace/* — correlation-id hop chain + observed integrations (extracted #2189)
import { fetchTraceByCorrelation, fetchTraceIntegrations } from './handlers/chorus-trace';
app.get('/api/chorus/trace/:correlationId', (req: Request, res: Response) => {
  if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    const r = fetchTraceByCorrelation(req.params.correlationId, { db });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});
app.get('/api/chorus/trace/integrations/:domain', (req: Request, res: Response) => {
  if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    const r = fetchTraceIntegrations(req.params.domain, { db });
    res.status(r.status).json(r.body);
  } finally { db.close(); }
});

process.on('uncaughtException', (err) => {
  console.error(`[chorus-api] FATAL uncaughtException: ${err.message}`);
  console.error(err.stack);
  crashAlert(err.message);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  const msg = reason instanceof Error ? reason.message : String(reason);
  console.error(`[chorus-api] FATAL unhandledRejection: ${msg}`);
  if (reason instanceof Error) console.error(reason.stack);
  crashAlert(msg);
  process.exit(1);
});

process.on('SIGTERM', () => {
  console.log(`[chorus-api] Received SIGTERM — shutting down`);
  process.exit(0);
});

// Only bind + start background timers when run as the main module. Under jest
// (require.main !== module) tests import `app` and exercise routes in-process
// (#2167 landed the listen guard; #2173 AC4 moved the timers in here too —
// setInterval/setTimeout at module-load kept jest alive forever, masking
// that the harness worked at all).
const BIND_HOST = process.env.CHORUS_BIND || '0.0.0.0';
if (require.main === module) {
  // Health cache refresh — runs every 30s under the live server only.
  setTimeout(() => refreshHealthCache(), 2000);
  setInterval(() => refreshHealthCache(), 30_000);

  // Scheduled reindex — live server only. First run after 60s startup delay.
  setTimeout(() => {
    scheduledReindex();
    setInterval(() => scheduledReindex(), REINDEX_INTERVAL);
  }, 60_000);

  app.listen(PORT, BIND_HOST, () => {
    console.log(`[chorus-api] Listening on ${BIND_HOST}:${PORT}`);
    console.log(`[chorus-api] Database: ${DB_PATH}`);
    // Init LanceDB async (non-blocking)
    initLance().catch(err => console.error(`[chorus-api] LanceDB init error: ${err}`));

    // Embed sync moved to standalone worker (chorus-embed-worker.sh) — #1978
    // The in-process timer was blocking the API with 100+ sequential Ollama calls per cycle.
    // POST /api/chorus/embed still works for on-demand batches.
  });
}

export default app;
