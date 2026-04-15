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

async function embedQuery(text: string): Promise<number[]> {
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

  return { domain_coverage: Math.round(domain_coverage * 100) / 100, newest_result_age_s, stale, sources };
}

// --- GET /api/chorus/search ---
// Supports mode=fts (default), mode=semantic, mode=hybrid

app.get('/api/chorus/search', async (req: Request, res: Response) => {
  const q = req.query.q as string;
  if (!q) {
    res.status(400).json({ error: 'Missing required parameter: q' });
    return;
  }

  const limit = Math.min(parseInt(req.query.limit as string || '20', 10), 100);
  const role = req.query.role as string | undefined;
  const mode = (req.query.mode as string || 'fts').toLowerCase();

  const searchStart = Date.now();

  // Semantic-only mode
  if (mode === 'semantic') {
    if (!lanceTable) {
      res.json({ results: [], total: 0, mode: 'semantic', error: 'Semantic index not available' });
      return;
    }
    try {
      const results = await semanticSearch(q, limit, role);
      emitSearchEvent({ system: 'chorus-api', query: q.slice(0, 200), mode: 'semantic', result_count: results.length, duration_ms: Date.now() - searchStart, ...(role ? { role_filter: role } : {}) });
      res.json({ results, total: results.length, mode: 'semantic', _meta: buildSearchMeta(results) });
    } catch (err) {
      res.status(500).json({ error: `Semantic search failed: ${err}` });
    }
    return;
  }

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    addStaleHeader(res, db);

    // FTS5 search — replace hyphens with spaces for tokenizer compat
    const ftsQuery = q.replace(/-/g, ' ');
    let roleFilter = '';
    const params: any[] = [ftsQuery, limit];
    if (role) {
      roleFilter = 'AND m.role = ?';
      params.splice(1, 0, role);
    }

    let ftsResults: any[];
    try {
      ftsResults = db.prepare(`
        SELECT m.id, m.source, m.channel, m.role, m.author, m.content, m.timestamp,
               snippet(messages_fts, 0, '<b>', '</b>', '...', 40) as snippet
        FROM messages_fts f
        JOIN messages m ON f.rowid = m.id
        WHERE messages_fts MATCH ?
        ${roleFilter}
        ORDER BY m.timestamp DESC
        LIMIT ?
      `).all(...params);
    } catch {
      // FTS5 syntax error — fall back to LIKE
      const likeParams: any[] = [`%${q}%`, limit];
      let likeRoleFilter = '';
      if (role) {
        likeRoleFilter = 'AND role = ?';
        likeParams.splice(1, 0, role);
      }
      ftsResults = db.prepare(`
        SELECT id, source, channel, role, author, content, timestamp, NULL as snippet
        FROM messages
        WHERE content LIKE ?
        ${likeRoleFilter}
        ORDER BY timestamp DESC
        LIMIT ?
      `).all(...likeParams);
    }

    // Unified mode: FTS + semantic + SPARQL via RRF
    if (mode === 'unified') {
      try {
        const [semResults, sparqlResults] = await Promise.all([
          lanceTable ? semanticSearch(q, limit, role) : Promise.resolve([]),
          sparqlSearch(q, limit),
        ]);
        const merged = mergeUnified(ftsResults, semResults, sparqlResults, limit);
        emitSearchEvent({ system: 'chorus-api', query: q.slice(0, 200), mode: 'unified', result_count: merged.length, sources: `fts=${ftsResults.length},semantic=${semResults.length},sparql=${sparqlResults.length}`, duration_ms: Date.now() - searchStart, ...(role ? { role_filter: role } : {}) });
        res.json({ results: merged, total: merged.length, mode: 'unified', sources: { fts: ftsResults.length, semantic: semResults.length, sparql: sparqlResults.length }, _meta: buildSearchMeta(merged, db) });
        return;
      } catch {
        // Fall through to FTS-only
      }
    }

    // Hybrid mode: merge FTS + semantic via RRF
    if (mode === 'hybrid' && lanceTable) {
      try {
        const semResults = await semanticSearch(q, limit, role);
        const merged = mergeRRF(ftsResults, semResults, limit);
        emitSearchEvent({ system: 'chorus-api', query: q.slice(0, 200), mode: 'hybrid', result_count: merged.length, duration_ms: Date.now() - searchStart, ...(role ? { role_filter: role } : {}) });
        res.json({ results: merged, total: merged.length, mode: 'hybrid', _meta: buildSearchMeta(merged, db) });
        return;
      } catch {
        // Semantic failed — fall through to FTS-only
      }
    }

    emitSearchEvent({ system: 'chorus-api', query: q.slice(0, 200), mode: 'fts', result_count: ftsResults.length, duration_ms: Date.now() - searchStart, ...(role ? { role_filter: role } : {}) });
    res.json({ results: ftsResults, total: ftsResults.length, mode: 'fts', _meta: buildSearchMeta(ftsResults, db) });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/conversation ---
// Returns a readable conversation thread between participants in a time range.
// Memory domain — team recall, not search. #1946

app.get('/api/chorus/conversation', (req: Request, res: Response) => {
  const rolesParam = req.query.roles as string;
  if (!rolesParam) {
    res.status(400).json({ error: 'Missing required parameter: roles (comma-separated, e.g. jeff,wren)' });
    return;
  }

  const roles = rolesParam.split(',').map(r => r.trim().toLowerCase());
  const date = req.query.date as string || new Date().toISOString().slice(0, 10);
  const tz = req.query.tz as string || 'America/New_York';
  const afterTime = req.query.after as string; // HH:MM in local tz
  const beforeTime = req.query.before as string; // HH:MM in local tz
  const limit = Math.min(parseInt(req.query.limit as string || '500', 10), 2000);

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    // Find sessions for the requested roles on the given date
    // A conversation between jeff and wren = messages where role is wren
    // (Jeff's words are author='user' in wren's session)
    const roleFilter = roles.filter(r => r !== 'jeff');
    if (roleFilter.length === 0) {
      res.status(400).json({ error: 'At least one non-jeff role required (jeff is always a participant via user messages)' });
      return;
    }

    const placeholders = roleFilter.map(() => '?').join(',');

    // Build time range — convert local time to query bounds
    let afterISO = `${date}T00:00:00`;
    let beforeISO = `${date}T23:59:59`;

    if (afterTime) {
      // Convert Boston time to UTC for query
      // Boston is UTC-4 (EDT) or UTC-5 (EST)
      const offsetHours = isEDT(date) ? 4 : 5;
      const [h, m] = afterTime.split(':').map(Number);
      const utcH = h + offsetHours;
      afterISO = `${date}T${String(utcH).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
    }
    if (beforeTime) {
      const offsetHours = isEDT(date) ? 4 : 5;
      const [h, m] = beforeTime.split(':').map(Number);
      const utcH = h + offsetHours;
      // Handle day rollover
      if (utcH >= 24) {
        const nextDate = new Date(date);
        nextDate.setDate(nextDate.getDate() + 1);
        const nd = nextDate.toISOString().slice(0, 10);
        beforeISO = `${nd}T${String(utcH - 24).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
      } else {
        beforeISO = `${date}T${String(utcH).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
      }
    }

    const rows = db.prepare(`
      SELECT author, content, timestamp, role, session_id
      FROM messages
      WHERE role IN (${placeholders})
      AND timestamp >= ?
      AND timestamp <= ?
      ORDER BY timestamp ASC
      LIMIT ?
    `).all(...roleFilter, afterISO, beforeISO, limit) as Array<{
      author: string; content: string; timestamp: string; role: string; session_id: string;
    }>;

    // Convert to conversation thread
    const thread = rows
      .filter(row => {
        const text = row.content.trim();
        // Filter out system noise
        if (text.startsWith('<system-reminder>')) return false;
        if (text.startsWith('<task-')) return false;
        if (text.startsWith('Base directory for this skill:')) return false;
        if (text.startsWith('[Request interrupted')) return false;
        if (text.length < 2) return false;
        return true;
      })
      .map(row => {
        const speaker = row.author === 'user' ? 'jeff' : row.role;
        const time = convertToLocal(row.timestamp, tz);
        return {
          speaker,
          text: row.content.trim(),
          time,
        };
      });

    // If time filter was in local time, re-filter by local time
    // (the UTC conversion is approximate — DST edge cases)
    let filteredThread = thread;
    if (afterTime || beforeTime) {
      filteredThread = thread.filter(msg => {
        const localHM = msg.time.split(' ')[1] || msg.time.slice(11, 16);
        if (afterTime && localHM < afterTime) return false;
        if (beforeTime && localHM >= beforeTime) return false;
        return true;
      });
    }

    res.json({
      thread: filteredThread,
      participants: roles,
      date,
      timezone: tz,
      count: filteredThread.length,
    });

  } finally {
    db.close();
  }
});

// --- GET /api/chorus/card-story/:id ---
// Memory domain — join six data sources into a card timeline. #1947

app.get('/api/chorus/card-story/:id', async (req: Request, res: Response) => {
  const cardId = parseInt(req.params.id, 10);
  if (isNaN(cardId)) {
    res.status(400).json({ error: 'Invalid card ID' });
    return;
  }

  const VIKUNJA_URL = 'http://localhost:3456';
  const VIKUNJA_TOKEN = process.env.VIKUNJA_TOKEN || '';
  const MESSAGING_URL = 'http://localhost:3475';

  const timeline: Array<{ timestamp: string; source: string; text: string; role?: string; event?: string }> = [];
  let title = '';
  let owner = '';
  let status = '';
  let domain = '';

  // 1. Card metadata + comments via cards CLI (boundary: never bypass to Vikunja)
  try {
    const cardsScript = path.resolve(__dirname, '../../scripts/cards');
    const { stdout: cardJson } = await execAsync(
      `bash ${cardsScript} view ${cardId} --json 2>/dev/null`,
      { encoding: 'utf-8', timeout: 10000, env: { ...process.env, PATH: `/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } }
    );
    const card = JSON.parse(cardJson);

    title = card.title || '';
    owner = (card.owner || '').toLowerCase();
    status = card.status || '';

    // Extract domain from domains array
    for (const d of card.domains || []) {
      const m = d.match(/domain:(\w+)/i);
      if (m) domain = m[1];
    }

    // Add comments to timeline
    for (const c of card.comments || []) {
      if (c.text && c.text.length > 5) {
        timeline.push({
          timestamp: c.created || card.created || '',
          source: 'vikunja',
          text: c.text.slice(0, 500),
          role: c.author,
        });
      }
    }
  } catch (e: any) {
    // Log the error so we can diagnose LaunchAgent PATH issues
    console.error(`[card-story] cards CLI failed for #${cardId}: ${e.message?.slice(0, 200)}`);
  }

  // 2. Chorus index mentions
  let db: Database.Database | null = null;
  try {
    db = getDb();
    const mentions = db.prepare(`
      SELECT author, content, timestamp, role
      FROM messages
      WHERE content LIKE ?
      ORDER BY timestamp ASC
      LIMIT 50
    `).all(`%#${cardId}%`) as Array<{ author: string; content: string; timestamp: string; role: string }>;

    for (const m of mentions) {
      const text = m.content.trim();
      if (text.startsWith('<system-reminder>')) continue;
      if (text.startsWith('Base directory for this skill:')) continue;
      if (text.length < 10) continue;
      timeline.push({
        timestamp: m.timestamp,
        source: 'chorus-index',
        text: text.slice(0, 500),
        role: m.author === 'user' ? 'jeff' : m.role,
      });
    }
  } catch { /* db not available */ }
  finally { if (db) db.close(); }

  // 3. Spine events from chorus.log
  try {
    const logPath = path.resolve(__dirname, '../../logs/chorus.log');
    if (fs.existsSync(logPath)) {
      const lines = fs.readFileSync(logPath, 'utf-8').split('\n');
      for (const line of lines) {
        if (!line.includes(`card=${cardId}`) && !line.includes(`"card":"${cardId}"`)) continue;
        try {
          const parsed = JSON.parse(line);
          if (parsed.event && parsed.event.startsWith('card.')) {
            timeline.push({
              timestamp: parsed.timestamp,
              source: 'spine',
              text: `${parsed.event}`,
              role: parsed.role,
              event: parsed.event,
            });
          }
        } catch { /* skip malformed lines */ }
      }
    }
  } catch { /* log not readable */ }

  // 4. Nudges referencing this card
  try {
    const nudgeResp = await fetch(`${MESSAGING_URL}/api/messages?limit=100`);
    if (nudgeResp.ok) {
      const messages = await nudgeResp.json() as Array<{ from: string; to: string; text: string; timestamp: string }>;
      for (const msg of messages) {
        if (msg.text && msg.text.includes(`#${cardId}`)) {
          timeline.push({
            timestamp: msg.timestamp,
            source: 'nudge',
            text: msg.text.slice(0, 500),
            role: msg.from,
          });
        }
      }
    }
  } catch { /* messaging tier not available */ }

  // Sort timeline chronologically
  timeline.sort((a, b) => a.timestamp.localeCompare(b.timestamp));

  res.json({
    card: cardId,
    title,
    owner,
    status,
    domain,
    timeline,
    sources: [...new Set(timeline.map(e => e.source))],
    count: timeline.length,
  });
});

// --- GET /api/chorus/domain-story/:domain ---
// Memory domain — institutional memory for a domain. Cards + conversation mentions + spine events. #1947

app.get('/api/chorus/domain-story/:domain', async (req: Request, res: Response) => {
  const domain = req.params.domain.toLowerCase();
  const limit = Math.min(parseInt(req.query.limit as string || '100', 10), 500);

  const cards: Array<{ index: number; title: string; status: string; owner: string; created: string }> = [];
  const mentions: Array<{ timestamp: string; role: string; text: string }> = [];
  const timeline: Array<{ timestamp: string; source: string; text: string; role?: string; card?: number }> = [];

  // 1. Cards tagged with this domain via cards CLI
  try {
    const cardsScript = path.resolve(__dirname, '../../scripts/cards');
    const { stdout: listOutput } = await execAsync(
      `bash ${cardsScript} list 2>/dev/null`,
      { encoding: 'utf-8', timeout: 15000, env: { ...process.env, PATH: `/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } }
    );

    for (const line of listOutput.split('\n')) {
      if (!line.includes(`domain:${domain}`)) continue;
      const indexMatch = line.match(/^\s*(\d+)/);
      if (!indexMatch) continue;
      const cardIndex = parseInt(indexMatch[1], 10);
      const titleMatch = line.match(/^\s*\d+\s+(.+?)\s*\[/);
      const statusMatch = line.match(/\[(WIP|Done|Next|Later|Won't Do|Blocked)/);
      const ownerMatch = line.match(/\[(\w+)\|/);

      const card = {
        index: cardIndex,
        title: titleMatch ? titleMatch[1].trim() : '',
        status: statusMatch ? statusMatch[1] : '',
        owner: ownerMatch ? ownerMatch[1].toLowerCase() : '',
        created: '',
      };
      cards.push(card);

      timeline.push({
        timestamp: '',
        source: 'card',
        text: `#${cardIndex} ${card.title} [${card.status}]`,
        role: card.owner,
        card: cardIndex,
      });
    }
  } catch { /* cards CLI failed */ }

  // 2. Conversation mentions from Chorus index
  let db: Database.Database | null = null;
  try {
    db = getDb();
    const rows = db.prepare(`
      SELECT author, content, timestamp, role
      FROM messages
      WHERE content LIKE ?
      ORDER BY timestamp ASC
      LIMIT ?
    `).all(`%${domain}%`, limit) as Array<{ author: string; content: string; timestamp: string; role: string }>;

    for (const m of rows) {
      const text = m.content.trim();
      if (text.startsWith('<system-reminder>')) continue;
      if (text.startsWith('Base directory for this skill:')) continue;
      if (text.length < 20) continue;

      const mention = {
        timestamp: m.timestamp,
        role: m.author === 'user' ? 'jeff' : m.role,
        text: text.slice(0, 300),
      };
      mentions.push(mention);

      timeline.push({
        timestamp: m.timestamp,
        source: 'chorus-index',
        text: text.slice(0, 300),
        role: mention.role,
      });
    }
  } catch { /* db not available */ }
  finally { if (db) db.close(); }

  // 3. Spine events for cards in this domain
  try {
    const logPath = path.resolve(__dirname, '../../logs/chorus.log');
    if (fs.existsSync(logPath)) {
      const lines = fs.readFileSync(logPath, 'utf-8').split('\n');
      const cardIds = new Set(cards.map(c => c.index));
      for (const line of lines) {
        try {
          const parsed = JSON.parse(line);
          if (!parsed.event || !parsed.event.startsWith('card.')) continue;
          const cardId = parseInt(parsed.card || '0', 10);
          if (!cardIds.has(cardId)) continue;
          timeline.push({
            timestamp: parsed.timestamp,
            source: 'spine',
            text: parsed.event,
            role: parsed.role,
            card: cardId,
          });
        } catch { /* skip */ }
      }
    }
  } catch { /* log not readable */ }

  // Sort timeline chronologically
  timeline.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));

  res.json({
    domain,
    cards,
    mentions,
    timeline,
    count: timeline.length,
    card_count: cards.length,
    mention_count: mentions.length,
  });
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

  // Source 1: Cards tagged with this domain (via cards CLI --json boundary)
  try {
    const cardsScript = path.resolve(__dirname, '../../scripts/cards');
    const { stdout: listOutput } = await execAsync(
      `bash ${cardsScript} list 2>/dev/null`,
      { encoding: 'utf-8', timeout: 15000, env: { ...process.env, PATH: `${NODE_PATH}:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } }
    );

    const domainCounts: Record<string, number> = {};

    for (const line of listOutput.split('\n')) {
      // Track all domains for related calculation
      const domainMatch = line.match(/domain:(\w+)/);
      if (domainMatch) {
        const d = domainMatch[1];
        domainCounts[d] = (domainCounts[d] || 0) + 1;
      }

      if (!line.includes(`domain:${domain}`)) continue;
      const indexMatch = line.match(/^\s*(\d+)/);
      if (!indexMatch) continue;
      const cardIndex = parseInt(indexMatch[1], 10);
      const titleMatch = line.match(/^\s*\d+\s+(.+?)\s*\[/);
      const statusMatch = line.match(/\[(WIP|Done|Next|Later|Won't Do|Blocked)/);
      const ownerMatch = line.match(/\[(\w+)\|/);

      // Get full card details via --json for accurate status + code files
      let cardStatus = '';
      let cardOwner = ownerMatch ? ownerMatch[1].toLowerCase() : '';
      try {
        const { stdout: cardJson } = await execAsync(
          `bash ${cardsScript} view ${cardIndex} --json 2>/dev/null`,
          { encoding: 'utf-8', timeout: 10000, env: { ...process.env, PATH: `${NODE_PATH}:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } }
        );
        const parsed = JSON.parse(cardJson);
        cardStatus = parsed.status || '';
        cardOwner = (parsed.owner || cardOwner).toLowerCase();

        // Extract file paths from description and comments
        const allText = (parsed.description || '') + ' ' + (parsed.comments || []).map((c: any) => c.text).join(' ');
        const fileMatches = allText.match(/[\w\-/.]+\.(ts|js|rs|sh|html|json|feature)/g) || [];
        for (const f of fileMatches) {
          if (!code.files.includes(f)) code.files.push(f);
          links.push({ from_type: 'card', from: cardIndex, to_type: 'code', to: f });
        }
      } catch { /* card detail failed */ }

      const card = {
        index: cardIndex,
        title: titleMatch ? titleMatch[1].trim() : '',
        status: cardStatus,
        owner: cardOwner,
      };
      cards.push(card);
      timeline.push({ timestamp: '', source: 'card', text: `#${cardIndex} ${card.title} [${card.status}]`, role: card.owner, card: cardIndex });

      // Track unresolved
      if (card.status !== 'Done' && card.status !== "Won't Do") {
        history.unresolved.push(card);
      }
    }

    // Related domains: any domain that shares conversation mentions
    // (calculated after mentions phase below)
  } catch { /* cards CLI failed */ }

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
app.get('/api/chorus/domain/:domain/code-files', async (req: Request, res: Response) => {
  const domain = req.params.domain.toLowerCase();
  const files: string[] = [];

  try {
    const domainSuffix = domain.endsWith('-domain') || domain.endsWith('-service') ? domain : `${domain}-domain`;
    const codeQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domainSuffix}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
    const codeResult = await athenaSparqlQuery(codeQuery);
    files.push(...codeResult.results.bindings.map((b: any) => b.filePath.value));

    // Also try -service suffix if no results
    if (files.length === 0 && !domain.endsWith('-service')) {
      const svcQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domain}-service> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
      const svcResult = await athenaSparqlQuery(svcQuery);
      files.push(...svcResult.results.bindings.map((b: any) => b.filePath.value));
    }
  } catch { /* graph query failed */ }

  res.json({ domain, files, count: files.length });
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
app.get('/api/chorus/domain/:name/code', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?file ?label ?filePath ?fileType ?description WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasCodeFile ?file . OPTIONAL { ?file rdfs:label ?label } OPTIONAL { ?file chorus:filePath ?filePath } OPTIONAL { ?file chorus:fileType ?fileType } OPTIONAL { ?file rdfs:comment ?description } } }`;
    const result = await athenaSparqlQuery(query);
    const allFiles = result.results.bindings.map((b: any) => ({
      path: b.filePath?.value || b.label?.value || b.file.value.split('#').pop(),
      type: b.fileType?.value || path.extname(b.filePath?.value || '').slice(1) || 'unknown',
      description: b.description?.value || null,
    }));
    const source = allFiles.filter((f: any) => !isTestFile(f.path));
    const byType = source.reduce((acc: Record<string, number>, f: any) => { acc[f.type] = (acc[f.type] || 0) + 1; return acc; }, {});
    res.json(athenaEnvelope('domain-code', { subdomain: sdId, files: source, byType }, Date.now() - start, { count: allFiles.length, source_count: source.length, test_count: allFiles.length - source.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-code', { subdomain: req.params.name, files: [], byType: {} }, Date.now() - start, { count: 0, source_count: 0, test_count: 0 }));
  }
});

// GET /api/chorus/domain/:name/tests — test files covering a domain (#2060 AC2)
app.get('/api/chorus/domain/:name/tests', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    // First: test files from code inventory
    const codeQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?filePath ?fileType WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . OPTIONAL { ?file chorus:fileType ?fileType } } }`;
    const codeResult = await athenaSparqlQuery(codeQuery);
    const codeTests = codeResult.results.bindings
      .filter((b: any) => isTestFile(b.filePath?.value || ''))
      .map((b: any) => ({ path: b.filePath.value, type: b.fileType?.value || path.extname(b.filePath.value).slice(1) || 'unknown' }));
    // Second: TestCoverage triples
    const tcQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?testFile ?testType WHERE { GRAPH <urn:chorus:instances> { ?tc a chorus:TestCoverage ; chorus:testFile ?testFile ; chorus:testType ?testType ; chorus:covers <${sdUri}> . } } ORDER BY ?testType ?testFile`;
    const tcResult = await athenaSparqlQuery(tcQuery);
    const coverageTests = tcResult.results.bindings.map((b: any) => ({ path: b.testFile.value, type: b.testType.value }));
    // Merge and deduplicate by path
    const seen = new Set<string>();
    const tests: Array<{ path: string; type: string }> = [];
    for (const t of [...codeTests, ...coverageTests]) {
      if (!seen.has(t.path)) { seen.add(t.path); tests.push(t); }
    }
    const byType: Record<string, number> = {};
    for (const t of tests) { byType[t.type] = (byType[t.type] || 0) + 1; }
    res.json(athenaEnvelope('domain-tests', { subdomain: sdId, tests, byType }, Date.now() - start, { count: tests.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-tests', { subdomain: req.params.name, tests: [], byType: {} }, Date.now() - start, { count: 0 }));
  }
});

// GET /api/chorus/domain/:name/alerts — alert rules for a domain (#2060 AC3)
app.get('/api/chorus/domain/:name/alerts', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    const domainLabel = sdId.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
    const ALERTS_DIR = path.join(REPO_ROOT, 'proving/domains/alerts');
    const alertFiles = fs.readdirSync(ALERTS_DIR).filter((f: string) => f.endsWith('.yml'));
    const alerts: any[] = [];
    for (const file of alertFiles) {
      const content = fs.readFileSync(path.join(ALERTS_DIR, file), 'utf-8');
      const lower = content.toLowerCase();
      if (lower.includes(domainLabel) || file.toLowerCase().includes(domainLabel)) {
        const name = content.match(/^name:\s*(.+)/m)?.[1]?.trim() || file.replace('.yml', '');
        const description = content.match(/^description:\s*(.+)/m)?.[1]?.trim() || '';
        const severity = content.match(/^severity:\s*(.+)/m)?.[1]?.trim() || 'unknown';
        const schedule = content.match(/^schedule:\s*"?(.+?)"?\s*$/m)?.[1]?.trim() || '';
        alerts.push({ file, name, description, severity, schedule });
      }
    }
    res.json(athenaEnvelope('domain-alerts', { subdomain: sdId, domainLabel, alerts }, Date.now() - start, { count: alerts.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-alerts', { subdomain: req.params.name, alerts: [] }, Date.now() - start, { count: 0 }));
  }
});

// GET /api/chorus/domain/:name/logs — log sources for a domain (#2060 AC4)
app.get('/api/chorus/domain/:name/logs', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?log ?label ?logPath ?logType WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasLogSource ?log . OPTIONAL { ?log rdfs:label ?label } OPTIONAL { ?log chorus:logPath ?logPath } OPTIONAL { ?log chorus:logType ?logType } } }`;
    const result = await athenaSparqlQuery(query);
    const logs = result.results.bindings.map((b: any) => ({
      label: b.label?.value || b.log.value.split('#').pop(),
      path: b.logPath?.value || null,
      type: b.logType?.value || 'unknown',
    }));
    res.json(athenaEnvelope('domain-logs', { subdomain: sdId, logs }, Date.now() - start, { count: logs.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-logs', { subdomain: req.params.name, logs: [] }, Date.now() - start, { count: 0 }));
  }
});

// GET /api/chorus/domain/:name/services — API endpoints in a domain (#2060 AC5)
app.get('/api/chorus/domain/:name/services', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?method ?routePath ?filePath WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasEndpoint ?ep . ?ep a chorus:Endpoint ; chorus:httpMethod ?method ; chorus:routePath ?routePath ; chorus:filePath ?filePath . } } ORDER BY ?method ?routePath`;
    const result = await athenaSparqlQuery(query);
    const endpoints = result.results.bindings.map((b: any) => ({
      method: b.method.value,
      path: b.routePath.value,
      handler: b.filePath.value,
    }));
    const byMethod: Record<string, number> = {};
    for (const e of endpoints) { byMethod[e.method] = (byMethod[e.method] || 0) + 1; }
    res.json(athenaEnvelope('domain-services', { subdomain: sdId, endpoints, byMethod }, Date.now() - start, { count: endpoints.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-services', { subdomain: req.params.name, endpoints: [], byMethod: {} }, Date.now() - start, { count: 0 }));
  }
});

// GET /api/chorus/domain/:name/infra — borg environments for a domain (#2080)
// Queries urn:borg:instances graph for domain-scoped environments via usesEnvironment edges.
app.get('/api/chorus/domain/:name/infra', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdId = await resolveSubdomainId(req.params.name);
    // Domain-scoped: follow usesEnvironment edges from the subdomain
    const query = `PREFIX borg: <urn:borg:ontology/>
PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?env ?name ?port ?health ?host ?engine ?dep WHERE {
  GRAPH <urn:borg:instances> {
    <https://jeffbridwell.com/chorus#${sdId}> borg:usesEnvironment ?env .
    ?env borg:environmentName ?name .
    OPTIONAL { ?env borg:port ?port }
    OPTIONAL { ?env borg:healthEndpoint ?health }
    OPTIONAL { ?env borg:runsOn/borg:hostName ?host }
    OPTIONAL { ?env borg:instanceOf/borg:engineName ?engine }
    OPTIONAL { ?env borg:dependsOn/borg:environmentName ?dep }
  }
} ORDER BY ?host ?name`;
    const result = await athenaSparqlQuery(query);
    // Group by environment, collect dependencies
    const envMap = new Map<string, any>();
    for (const b of result.results.bindings) {
      const envName = b.name.value;
      if (!envMap.has(envName)) {
        envMap.set(envName, {
          name: envName,
          port: b.port?.value || null,
          health: b.health?.value || null,
          host: b.host?.value || null,
          engine: b.engine?.value || null,
          dependsOn: [],
        });
      }
      if (b.dep?.value && !envMap.get(envName).dependsOn.includes(b.dep.value)) {
        envMap.get(envName).dependsOn.push(b.dep.value);
      }
    }
    const environments = Array.from(envMap.values());
    res.json(athenaEnvelope('domain-infra', { subdomain: req.params.name, environments }, Date.now() - start, { count: environments.length }));
  } catch (err: any) {
    res.json(athenaEnvelope('domain-infra', { subdomain: req.params.name, environments: [] }, Date.now() - start, { count: 0 }));
  }
});

// GET /api/chorus/domain/:name/pipeline — value stream lifecycle (#2069)
// Assembles from 5 existing sources: cards, completeness, code/tests/endpoints, alerts/gates, done cards.
app.get('/api/chorus/domain/:name/pipeline', async (req: Request, res: Response) => {
  const start = Date.now();
  const name = req.params.name;
  try {
    const sdId = await resolveSubdomainId(name);
    const domainLabel = sdId.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();

    // Parallel fetch from all existing sources
    const [cardsRes, compRes, codeRes, testsRes, endpointsRes, alertsRes] = await Promise.all([
      fetch(`http://localhost:3340/api/athena/subdomains/${sdId}/cards`).then(r => r.ok ? r.json() : null).catch(() => null),
      fetch(`http://localhost:3340/api/athena/subdomains/${sdId}/completeness`).then(r => r.ok ? r.json() : null).catch(() => null),
      fetch(`http://localhost:3340/api/chorus/domain/${name}/code`).then(r => r.ok ? r.json() : null).catch(() => null),
      fetch(`http://localhost:3340/api/chorus/domain/${name}/tests`).then(r => r.ok ? r.json() : null).catch(() => null),
      fetch(`http://localhost:3340/api/chorus/domain/${name}/services`).then(r => r.ok ? r.json() : null).catch(() => null),
      fetch(`http://localhost:3340/api/chorus/domain/${name}/alerts`).then(r => r.ok ? r.json() : null).catch(() => null),
    ]);

    // Extract counts
    const cards = (cardsRes as any)?.data?.cards || (cardsRes as any)?.data || [];
    const totalCards = cards.length;
    const doneCards = cards.filter((c: any) => c.status === 'Done').length;
    const wipCards = cards.filter((c: any) => c.status === 'WIP').length;
    const completeness = (compRes as any)?.data?.percentage ?? 0;
    const compPresent = (compRes as any)?.data?.present || [];
    const compMissing = (compRes as any)?.data?.missing || [];
    const codeCount = (codeRes as any)?._meta?.source_count ?? 0;
    const testCount = (testsRes as any)?._meta?.count ?? 0;
    const endpointCount = (endpointsRes as any)?._meta?.count ?? 0;
    const alertCount = (alertsRes as any)?._meta?.count ?? 0;

    // Build evidence counts
    const buildEvidence = codeCount + testCount + endpointCount;

    // Determine status per stage
    const stageStatus = (evidence: number, threshold: number = 1) =>
      evidence === 0 ? 'not_started' : evidence >= threshold ? 'complete' : 'in_progress';

    const stages = [
      {
        name: 'shape',
        status: totalCards === 0 ? 'not_started' : totalCards >= 5 ? 'complete' : 'in_progress',
        evidence: totalCards,
        detail: { total_cards: totalCards, wip: wipCards, done: doneCards },
        summary: totalCards === 0 ? 'No cards' : `${totalCards} cards (${wipCards} WIP, ${doneCards} done)`,
      },
      {
        name: 'design',
        status: completeness === 0 ? 'not_started' : completeness >= 80 ? 'complete' : 'in_progress',
        evidence: completeness,
        detail: { percentage: completeness, present: compPresent, missing: compMissing },
        summary: completeness === 0 ? 'Not started' : `${completeness}% — ${compPresent.length} present, ${compMissing.length} missing`,
      },
      {
        name: 'build',
        status: stageStatus(buildEvidence, 3),
        evidence: buildEvidence,
        detail: { code: codeCount, tests: testCount, endpoints: endpointCount },
        summary: buildEvidence === 0 ? 'No code discovered' : `${codeCount} source, ${testCount} tests, ${endpointCount} endpoints`,
      },
      {
        name: 'prove',
        status: stageStatus(alertCount),
        evidence: alertCount,
        detail: { alerts: alertCount },
        summary: alertCount === 0 ? 'No alert coverage' : `${alertCount} alert rules`,
      },
      {
        name: 'ship',
        status: doneCards === 0 ? 'not_started' : doneCards >= totalCards * 0.5 ? 'complete' : 'in_progress',
        evidence: doneCards,
        detail: { done: doneCards, total: totalCards, ratio: totalCards > 0 ? Math.round(doneCards / totalCards * 100) : 0 },
        summary: doneCards === 0 ? 'Nothing shipped' : `${doneCards}/${totalCards} cards shipped (${totalCards > 0 ? Math.round(doneCards / totalCards * 100) : 0}%)`,
      },
    ];

    res.json(athenaEnvelope('domain-pipeline', { subdomain: sdId, stages }, Date.now() - start, { count: 5 }));
  } catch (err: any) {
    // Return empty pipeline, not error
    const emptyStages = ['shape', 'design', 'build', 'prove', 'ship'].map(name => ({
      name, status: 'not_started', evidence: 0, detail: {}, summary: 'No data',
    }));
    res.json(athenaEnvelope('domain-pipeline', { subdomain: name, stages: emptyStages }, Date.now() - start, { count: 5 }));
  }
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
function mergeRRF(ftsResults: any[], semResults: SemanticResult[], limit: number, k = 60): any[] {
  const scoreMap = new Map<number, { score: number; result: any }>();

  // Score FTS results by rank position
  ftsResults.forEach((r, i) => {
    const key = r.id || r.msg_id;
    const rrfScore = 1 / (k + i + 1);
    scoreMap.set(key, { score: rrfScore, result: r });
  });

  // Score semantic results and merge
  semResults.forEach((r, i) => {
    const key = r.msg_id;
    const rrfScore = 1 / (k + i + 1);
    const existing = scoreMap.get(key);
    if (existing) {
      existing.score += rrfScore; // boosted by appearing in both
    } else {
      scoreMap.set(key, {
        score: rrfScore,
        result: {
          source: r.source,
          channel: r.channel,
          role: r.role,
          content: r.content,
          timestamp: r.timestamp,
          snippet: null,
          _semantic_score: r.score,
        }
      });
    }
  });

  return Array.from(scoreMap.values())
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map(e => ({ ...e.result, _rrf_score: e.score }));
}

// --- GET /api/chorus/reconcile ---

app.get('/api/chorus/reconcile', (req: Request, res: Response) => {
  const role = req.query.role as string;
  if (!role || !['wren', 'silas', 'kade'].includes(role)) {
    res.status(400).json({ error: 'Missing or invalid parameter: role (wren|silas|kade)' });
    return;
  }

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    addStaleHeader(res, db);

    // Find last session end for this role
    const lastSession = db.prepare(`
      SELECT MAX(timestamp) as ts FROM messages
      WHERE source = 'claude' AND channel = ? AND author = 'assistant'
    `).get(`session:${role}`) as { ts: string | null };

    // Default to 24h ago if no prior session
    let cutoff: string;
    if (lastSession?.ts) {
      cutoff = lastSession.ts;
    } else {
      cutoff = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();
    }

    // Slack messages since cutoff (grouped by channel)
    const slackMessages = db.prepare(`
      SELECT channel, role, author, content, timestamp FROM messages
      WHERE source = 'slack' AND timestamp > ?
        AND NOT (is_bridge = 1 AND role = ?)
      ORDER BY timestamp ASC
    `).all(cutoff, role) as any[];

    // Group slack by channel, limit 5 per channel
    const slackByChannel: Record<string, any[]> = {};
    for (const msg of slackMessages) {
      if (!slackByChannel[msg.channel]) slackByChannel[msg.channel] = [];
      if (slackByChannel[msg.channel].length < 5) {
        slackByChannel[msg.channel].push(msg);
      }
    }

    // Other roles' session counts
    const sessionRows = db.prepare(`
      SELECT channel, COUNT(*) as count FROM messages
      WHERE source = 'claude' AND channel != ? AND timestamp > ?
      GROUP BY channel
    `).all(`session:${role}`, cutoff) as any[];

    const sessions: Record<string, number> = {};
    for (const row of sessionRows) {
      const roleName = row.channel.replace('session:', '');
      sessions[roleName] = row.count;
    }

    // Jeff's direction (user messages in other role sessions)
    const jeffDirection = db.prepare(`
      SELECT channel, content, timestamp FROM messages
      WHERE source = 'claude' AND author = 'user' AND timestamp > ?
      ORDER BY timestamp DESC
      LIMIT 10
    `).all(cutoff) as any[];

    // Stats
    const total = (db.prepare(`SELECT COUNT(*) as c FROM messages`).get() as any).c;
    const bySourceRows = db.prepare(
      `SELECT source, COUNT(*) as c FROM messages GROUP BY source`
    ).all() as any[];
    const bySource: Record<string, number> = {};
    for (const row of bySourceRows) bySource[row.source] = row.c;

    res.json({
      slack: slackByChannel,
      sessions,
      jeffDirection,
      stats: { total, bySource }
    });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/refs ---

app.get('/api/chorus/refs', (req: Request, res: Response) => {
  const card = req.query.card as string | undefined;
  const wf = req.query.wf as string | undefined;
  const type = req.query.type as string | undefined;
  const entityId = req.query.id as string | undefined;

  // At least one filter required
  if (!card && !wf && !type && !entityId) {
    res.status(400).json({ error: 'At least one filter required: card, wf, type, or id' });
    return;
  }

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    addStaleHeader(res, db);

    let where: string[] = [];
    let params: any[] = [];

    if (card) {
      where.push('r.entity_type = ? AND r.entity_id = ?');
      // entity_id stores with # prefix (e.g., "#30")
      params.push('card', card.startsWith('#') ? card : `#${card}`);
    } else if (wf) {
      where.push('r.entity_type = ? AND r.entity_id = ?');
      // entity_id stores with WF- prefix (e.g., "WF-004")
      params.push('workflow', wf.startsWith('WF-') ? wf : `WF-${wf}`);
    } else {
      if (type) { where.push('r.entity_type = ?'); params.push(type); }
      if (entityId) { where.push('r.entity_id = ?'); params.push(entityId); }
    }

    const refs = db.prepare(`
      SELECT r.entity_type, r.entity_id, r.relationship,
             m.content, m.timestamp, m.role, m.source, m.channel
      FROM refs r
      JOIN messages m ON r.message_id = m.id
      WHERE ${where.join(' AND ')}
      ORDER BY m.timestamp DESC
      LIMIT 50
    `).all(...params) as any[];

    const formatted = refs.map(r => ({
      entity_type: r.entity_type,
      entity_id: r.entity_id,
      relationship: r.relationship,
      message: {
        content: r.content?.substring(0, 500),
        timestamp: r.timestamp,
        role: r.role,
        source: r.source,
        channel: r.channel
      }
    }));

    res.json({ refs: formatted });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/stats ---

app.get('/api/chorus/stats', (_req: Request, res: Response) => {
  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    addStaleHeader(res, db);

    const total = (db.prepare(`SELECT COUNT(*) as c FROM messages`).get() as any).c;

    const bySourceRows = db.prepare(
      `SELECT source, COUNT(*) as c FROM messages GROUP BY source ORDER BY c DESC`
    ).all() as any[];
    const bySource: Record<string, number> = {};
    for (const row of bySourceRows) bySource[row.source] = row.c;

    const byRoleRows = db.prepare(
      `SELECT role, COUNT(*) as c FROM messages GROUP BY role ORDER BY c DESC`
    ).all() as any[];
    const byRole: Record<string, number> = {};
    for (const row of byRoleRows) byRole[row.role] = row.c;

    const dateRange = db.prepare(
      `SELECT MIN(timestamp) as earliest, MAX(timestamp) as latest FROM messages`
    ).get() as any;

    const watermarks = db.prepare(
      `SELECT source, last_indexed FROM watermarks ORDER BY last_indexed DESC`
    ).all() as any[];

    const lastIndexed = watermarks.length > 0 ? watermarks[0].last_indexed : null;

    const refCount = (db.prepare(`SELECT COUNT(*) as c FROM refs`).get() as any).c;

    res.json({
      total,
      bySource,
      byRole,
      dateRange: { earliest: dateRange.earliest, latest: dateRange.latest },
      lastIndexed,
      watermarks,
      refs: refCount
    });
  } finally {
    db.close();
  }
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

app.get('/api/chorus/freshness', (_req: Request, res: Response) => {
  if (!fs.existsSync(DB_PATH)) {
    res.status(503).json({ error: 'Index database not found' });
    return;
  }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');

  try {
    const watermarks = db.prepare(
      `SELECT source, last_indexed FROM watermarks ORDER BY source`
    ).all() as Array<{ source: string; last_indexed: string }>;

    const now = Date.now();
    // Aggregate by source prefix — watermarks has per-file entries (artifact:adr:ADR-001...)
    // Roll up to source type level (claude, spine, brief, artifact:adr, etc.)
    const aggregated = new Map<string, string>();
    for (const w of watermarks) {
      const parts = w.source.split(':');
      const key = parts[0] === 'artifact' ? parts.slice(0, 2).join(':') : parts[0];
      const existing = aggregated.get(key);
      if (!existing || w.last_indexed > existing) {
        aggregated.set(key, w.last_indexed);
      }
    }

    // Drift counts for countable sources (#1959)
    // Count indexed vs total session messages (#1959)
    const claudeIndexed = (db.prepare("SELECT COUNT(*) as cnt FROM messages WHERE source='claude'").get() as { cnt: number }).cnt;
    // On-disk count: approximate from watermark coverage vs total unique sessions
    const claudeWatermarks = (db.prepare("SELECT COUNT(*) as cnt FROM watermarks WHERE source LIKE 'claude:%'").get() as { cnt: number }).cnt;
    const claudeOnDisk = claudeWatermarks; // When fully indexed, these match — drift = 0
    const spineOnDisk = fs.existsSync(path.join(REPO_ROOT, 'platform/logs/chorus.log'))
      ? fs.readFileSync(path.join(REPO_ROOT, 'platform/logs/chorus.log'), 'utf-8').split('\n').length
      : 0;
    const spineIndexed = (db.prepare("SELECT COUNT(*) as cnt FROM messages WHERE source='spine'").get() as { cnt: number }).cnt;

    const driftMap: Record<string, { onDisk: number; indexed: number }> = {
      claude: { onDisk: claudeOnDisk, indexed: claudeIndexed },
      spine: { onDisk: spineOnDisk, indexed: spineIndexed },
    };

    const sources = Array.from(aggregated.entries()).map(([source, lastIndexed]) => {
      const lastMs = new Date(lastIndexed).getTime();
      const ageSecs = Math.floor((now - lastMs) / 1000);
      const cadenceKey = source.split(':')[0];
      const cadence = SOURCE_CADENCE[cadenceKey] || SOURCE_CADENCE[source] || 86400;
      const ratio = ageSecs / cadence;

      const drift = driftMap[cadenceKey];
      let level: string;
      let unindexed = 0;
      if (drift) {
        unindexed = Math.max(0, drift.onDisk - drift.indexed);
        if (unindexed === 0) level = 'fresh';
        else if (unindexed < 100) level = 'warn';
        else if (unindexed < 1000) level = 'critical';
        else level = 'dead';
      } else {
        if (ratio <= 1.5) level = 'fresh';
        else if (ratio <= 3) level = 'warn';
        else if (ratio <= 7) level = 'critical';
        else level = 'dead';
      }

      return {
        source,
        last_indexed: lastIndexed,
        age_seconds: ageSecs,
        expected_cadence: cadence,
        staleness_ratio: Math.round(ratio * 10) / 10,
        unindexed,
        level,
      };
    });

    const summary = {
      total_sources: sources.length,
      fresh: sources.filter(s => s.level === 'fresh').length,
      warn: sources.filter(s => s.level === 'warn').length,
      critical: sources.filter(s => s.level === 'critical').length,
      dead: sources.filter(s => s.level === 'dead').length,
    };

    res.json({ sources, summary, timestamp: bostonNow() });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/pulse/latest (#1881) ---
// Returns most recent Pulse team state snapshot

app.get('/api/chorus/pulse/latest', (_req: Request, res: Response) => {
  const pulsePath = '/tmp/pulse-latest.json';
  try {
    if (!fs.existsSync(pulsePath)) {
      res.status(404).json({ error: 'No pulse snapshot available. Run chorus-hook-shim pulse first.' });
      return;
    }
    const content = fs.readFileSync(pulsePath, 'utf-8');
    const pulse = JSON.parse(content);
    res.json(pulse);
  } catch (err: any) {
    res.status(500).json({ error: err.message });
  }
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

app.get('/api/chorus/self', async (req: Request, res: Response) => {
  const q = req.query.q as string;
  if (!q) {
    res.status(400).json({ error: 'Missing required parameter: q' });
    return;
  }

  const limit = Math.min(parseInt(req.query.limit as string || '10', 10), 50);
  const searchStart = Date.now();

  let db: Database.Database;
  try {
    db = getDb();
  } catch (e) {
    if (e instanceof DbNotFoundError) { res.status(503).json({ error: e.message }); return; }
    throw e;
  }

  try {
    // FTS with source filter — only whitelisted sources
    const sourceList = Array.from(SELF_SOURCE_WHITELIST).map(s => `'${s}'`).join(',');
    const ftsQuery = q.replace(/-/g, ' ');
    let ftsResults: any[];
    try {
      ftsResults = db.prepare(`
        SELECT m.id, m.source, m.channel, m.role, m.content, m.timestamp,
               snippet(messages_fts, 0, '<b>', '</b>', '...', 40) as snippet
        FROM messages_fts f
        JOIN messages m ON f.rowid = m.id
        WHERE messages_fts MATCH ?
        AND m.source IN (${sourceList})
        ORDER BY m.timestamp DESC
        LIMIT ?
      `).all(ftsQuery, limit);
    } catch {
      ftsResults = db.prepare(`
        SELECT id, source, channel, role, content, timestamp, NULL as snippet
        FROM messages
        WHERE content LIKE ?
        AND source IN (${sourceList})
        ORDER BY timestamp DESC
        LIMIT ?
      `).all(`%${q}%`, limit);
    }

    // Semantic search — filter results to whitelisted sources
    let semResults: SemanticResult[] = [];
    if (lanceTable) {
      try {
        const rawSem = await semanticSearch(q, limit * 3); // over-fetch, then filter
        semResults = rawSem.filter(r => SELF_SOURCE_WHITELIST.has(r.source)).slice(0, limit);
      } catch { /* semantic unavailable */ }
    }

    // SPARQL — always included (RDF entities are curated, not raw sessions)
    let sparqlResults: SparqlResult[] = [];
    try {
      sparqlResults = await sparqlSearch(q, limit);
    } catch { /* sparql unavailable */ }

    // Merge via RRF
    const merged = mergeUnified(ftsResults, semResults, sparqlResults, limit);

    emitSearchEvent({
      system: 'chorus-self', query: q.slice(0, 200), mode: 'self',
      result_count: merged.length,
      sources: `fts=${ftsResults.length},semantic=${semResults.length},sparql=${sparqlResults.length}`,
      duration_ms: Date.now() - searchStart,
    });

    res.json({
      results: merged,
      total: merged.length,
      mode: 'self',
      sources: { fts: ftsResults.length, semantic: semResults.length, sparql: sparqlResults.length },
      filter: { whitelist: Array.from(SELF_SOURCE_WHITELIST) },
    });
  } finally {
    db.close();
  }
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

app.get('/api/chorus/reprompt-analytics', (req: Request, res: Response) => {
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

    const filtered = rows.map(r => {
      const cleaned = r.content.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, '').trim();
      return { ...r, content: cleaned };
    }).filter(r => {
      const c = r.content;
      if (!c || c.length > 500) return false;
      if (c.startsWith('<') || c.startsWith('{') || c.startsWith('[')) return false;
      if (/^\/Users\/|^\/tmp\/|^\/opt\//.test(c)) return false;
      if (/^(exit|y|n|yes|no)$/i.test(c.trim())) return false;
      return true;
    });

    // Re-prompt signal patterns
    const REPROMPT_KEYWORDS = /\bagain\b|\bstill\b|\balready said\b|\btold you\b|\bsame thing\b|\blike i said\b|\brepeat\b|\bi just said\b|\bjust told\b|\bsaid this\b/i;
    const APPROVAL_BIGRAMS = /\byes please\b|\byes go\b|\byes do\b|\bgo ahead\b|\byes that\b|\byes card\b/i;
    const CORRECTION_PATTERNS = /\bno[\s,]|\bwrong\b|\bthat'?s not\b|\bnot what i\b|\bstop\b|\bdon'?t\b|\bnever\b/i;

    // Classify each message
    type RepromptEvent = {
      text: string;
      role: string;
      timestamp: string;
      type: 'reprompt' | 'approval' | 'correction';
    };

    const events: RepromptEvent[] = [];
    const dailyCounts: Record<string, { reprompt: number; approval: number; correction: number; total: number }> = {};
    const roleCounts: Record<string, { reprompt: number; approval: number; correction: number }> = {
      wren: { reprompt: 0, approval: 0, correction: 0 },
      silas: { reprompt: 0, approval: 0, correction: 0 },
      kade: { reprompt: 0, approval: 0, correction: 0 },
    };

    for (const r of filtered) {
      const text = r.content.toLowerCase().trim();
      const role = r.channel.replace('session:', '') as string;
      const day = r.timestamp.substring(0, 10);
      if (!dailyCounts[day]) dailyCounts[day] = { reprompt: 0, approval: 0, correction: 0, total: 0 };
      dailyCounts[day].total++;

      let type: RepromptEvent['type'] | null = null;

      if (REPROMPT_KEYWORDS.test(text)) {
        // Filter false positives: "run it again" after a test is not a re-prompt
        if (!/run.*(again|it)|try.*(again|it)|again\?$/.test(text)) {
          type = 'reprompt';
        }
      } else if (APPROVAL_BIGRAMS.test(text)) {
        type = 'approval' as RepromptEvent['type'];
      } else if (CORRECTION_PATTERNS.test(text) && text.length < 100) {
        type = 'correction';
      }

      if (type) {
        events.push({ text: r.content.substring(0, 120), role, timestamp: r.timestamp, type });
        dailyCounts[day][type]++;
        if (roleCounts[role]) roleCounts[role][type]++;
      }
    }

    // Build daily trend (sorted)
    const sortedDays = Object.keys(dailyCounts).sort();
    const trend = sortedDays.map(day => ({
      date: day,
      ...dailyCounts[day],
      attentionCost: dailyCounts[day].reprompt * 3 + dailyCounts[day].approval + dailyCounts[day].correction * 2,
    }));

    // Headline stats
    const totalMessages = filtered.length;
    const totalSignals = events.length;
    const repromptCount = events.filter(e => e.type === 'reprompt').length;
    const approvalCount = events.filter(e => e.type === 'approval').length;
    const correctionCount = events.filter(e => e.type === 'correction').length;

    // Top re-prompt phrases (normalized)
    const phraseCount: Record<string, number> = {};
    for (const e of events) {
      const normalized = e.text.toLowerCase().trim().replace(/[^a-z0-9\s]/g, '').substring(0, 60);
      phraseCount[normalized] = (phraseCount[normalized] || 0) + 1;
    }
    const topPhrases = Object.entries(phraseCount)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 15)
      .map(([phrase, count]) => ({ phrase, count }));

    res.json({
      headline: {
        totalMessages,
        totalSignals,
        signalRate: totalMessages > 0 ? Math.round((totalSignals / totalMessages) * 100) : 0,
        reprompt: repromptCount,
        approvalOverhead: approvalCount,
        correction: correctionCount,
      },
      byRole: roleCounts,
      trend,
      topPhrases,
      recentEvents: events.slice(-20).reverse(),
      meta: { days, cutoff, messagesAnalyzed: filtered.length },
    });
  } finally {
    db.close();
  }
});

// --- GET /api/chorus/attention-analytics ---

app.get('/api/chorus/attention-analytics', (_req: Request, res: Response) => {
  const TSV_PATH = '/tmp/claude-team-scan/jeff-intensity-history.tsv';
  const STATE_PATH = '/tmp/claude-team-scan/jeff-state.json';
  const PROMPT_DIR = '/tmp/claude-team-scan';

  try {
    // Read current state
    let current: any = null;
    if (fs.existsSync(STATE_PATH)) {
      current = JSON.parse(fs.readFileSync(STATE_PATH, 'utf-8'));
    }

    // Read TSV history
    if (!fs.existsSync(TSV_PATH)) {
      res.json({ current, history: [], meta: { rows: 0, error: 'No history file' } });
      return;
    }

    const raw = fs.readFileSync(TSV_PATH, 'utf-8').trim().split('\n');
    if (raw.length < 2) {
      res.json({ current, history: [], meta: { rows: 0 } });
      return;
    }

    const headers = raw[0].split('\t');
    const rows = raw.slice(1).map(line => {
      const cols = line.split('\t');
      const row: Record<string, any> = {};
      headers.forEach((h, i) => {
        const v = cols[i];
        row[h] = h === 'intensity' ? v : parseFloat(v) || 0;
      });
      return row;
    });

    const now = Date.now() / 1000;

    // --- Headline metrics (last 24h) ---
    const day = rows.filter(r => now - r.timestamp < 86400);
    const activeDay = day.filter(r => r.keys_per_min > 0 || r.prompts_1h > 0);
    const avgPromptsHr = activeDay.length > 0
      ? activeDay.reduce((s, r) => s + r.prompts_1h, 0) / activeDay.length
      : 0;
    const avgKeysMin = activeDay.length > 0
      ? activeDay.reduce((s, r) => s + r.keys_per_min, 0) / activeDay.length
      : 0;
    const avgBreaks3h = activeDay.length > 0
      ? activeDay.reduce((s, r) => s + r.break_count_3h, 0) / activeDay.length
      : 0;
    const intensityCounts = { green: 0, yellow: 0, red: 0 };
    day.forEach(r => { if (intensityCounts[r.intensity as keyof typeof intensityCounts] !== undefined) intensityCounts[r.intensity as keyof typeof intensityCounts]++; });
    const dayTotal = day.length || 1;

    const headline = {
      avgPromptsHr: Math.round(avgPromptsHr),
      avgKeysMin: Math.round(avgKeysMin),
      avgBreaks3h: Math.round(avgBreaks3h * 10) / 10,
      greenPct: Math.round(intensityCounts.green / dayTotal * 100),
      yellowPct: Math.round(intensityCounts.yellow / dayTotal * 100),
      redPct: Math.round(intensityCounts.red / dayTotal * 100),
    };

    // --- Daily rhythm: hour-of-day buckets (Boston — DST-aware) ---
    const hourBuckets: { keys: number[]; prompts: number[]; count: number[] } = {
      keys: Array(24).fill(0),
      prompts: Array(24).fill(0),
      count: Array(24).fill(0),
    };
    day.forEach(r => {
      const d = new Date(r.timestamp * 1000);
      const off = isEDT(d.toISOString().slice(0, 10)) ? 4 : 5;
      const hr = (d.getUTCHours() - off + 24) % 24;
      hourBuckets.keys[hr] += r.keys_per_min;
      hourBuckets.prompts[hr] += r.prompts_1h;
      hourBuckets.count[hr]++;
    });
    const hourOfDay = Array.from({ length: 24 }, (_, i) => ({
      hour: i,
      avgKeys: hourBuckets.count[i] ? Math.round(hourBuckets.keys[i] / hourBuckets.count[i]) : 0,
      avgPrompts: hourBuckets.count[i] ? Math.round(hourBuckets.prompts[i] / hourBuckets.count[i]) : 0,
    }));

    // --- Intensity timeline: 15-min bands over last 7 days ---
    const bandSize = 900; // 15 minutes
    const bands: { timestamp: number; green: number; yellow: number; red: number }[] = [];
    if (rows.length > 0) {
      let bandStart = rows[0].timestamp;
      let g = 0, y = 0, r2 = 0;
      for (const r of rows) {
        if (r.timestamp - bandStart >= bandSize) {
          bands.push({ timestamp: bandStart, green: g, yellow: y, red: r2 });
          bandStart = r.timestamp;
          g = 0; y = 0; r2 = 0;
        }
        if (r.intensity === 'green') g++;
        else if (r.intensity === 'yellow') y++;
        else if (r.intensity === 'red') r2++;
      }
      bands.push({ timestamp: bandStart, green: g, yellow: y, red: r2 });
    }

    // --- Break patterns: daily break count and longest break ---
    const dailyBreaks: Record<string, { breaks: number[]; longest: number[] }> = {};
    rows.forEach(r => {
      const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
      if (!dailyBreaks[d]) dailyBreaks[d] = { breaks: [], longest: [] };
      dailyBreaks[d].breaks.push(r.break_count_3h);
      dailyBreaks[d].longest.push(r.longest_break_min);
    });
    const breakPatterns = Object.entries(dailyBreaks).sort().map(([date, data]) => ({
      date,
      avgBreaks: Math.round(Math.max(...data.breaks) * 10) / 10,
      longestBreak: Math.max(...data.longest),
    }));

    // --- Role attention: per-role prompt counts from log files ---
    const roleAttention: Record<string, number[]> = { wren: [], silas: [], kade: [] };
    for (const role of ['wren', 'silas', 'kade']) {
      const logPath = path.join(PROMPT_DIR, `${role}-prompt-times.log`);
      if (fs.existsSync(logPath)) {
        const times = fs.readFileSync(logPath, 'utf-8').trim().split('\n')
          .map(t => parseInt(t, 10)).filter(t => !isNaN(t));
        // Bucket into hours for last 24h
        const hourCounts = Array(24).fill(0);
        times.forEach(t => {
          if (now - t < 86400) {
            const hr = new Date((t - 5 * 3600) * 1000).getUTCHours();
            hourCounts[hr]++;
          }
        });
        roleAttention[role] = hourCounts;
      }
    }

    // --- Typing vs prompting: 30-min windows over last 24h ---
    const windowSize = 1800;
    const typingVsPrompting: { timestamp: number; keysAvg: number; promptsAvg: number }[] = [];
    if (day.length > 0) {
      let wStart = day[0].timestamp;
      let kSum = 0, pSum = 0, cnt = 0;
      for (const r of day) {
        if (r.timestamp - wStart >= windowSize) {
          typingVsPrompting.push({
            timestamp: wStart,
            keysAvg: cnt ? Math.round(kSum / cnt) : 0,
            promptsAvg: cnt ? Math.round(pSum / cnt) : 0,
          });
          wStart = r.timestamp;
          kSum = 0; pSum = 0; cnt = 0;
        }
        kSum += r.keys_per_min;
        pSum += r.prompts_1h;
        cnt++;
      }
      if (cnt > 0) {
        typingVsPrompting.push({
          timestamp: wStart,
          keysAvg: Math.round(kSum / cnt),
          promptsAvg: Math.round(pSum / cnt),
        });
      }
    }

    // --- Daily summary: per-day aggregates ---
    const dailySummary: Record<string, { active: number; total: number; peakKeys: number; peakPrompts: number; redMin: number }> = {};
    rows.forEach(r => {
      const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
      if (!dailySummary[d]) dailySummary[d] = { active: 0, total: 0, peakKeys: 0, peakPrompts: 0, redMin: 0 };
      dailySummary[d].total++;
      if (r.keys_per_min > 0 || r.prompts_1h > 0) dailySummary[d].active++;
      if (r.keys_per_min > dailySummary[d].peakKeys) dailySummary[d].peakKeys = r.keys_per_min;
      if (r.prompts_1h > dailySummary[d].peakPrompts) dailySummary[d].peakPrompts = r.prompts_1h;
      if (r.intensity === 'red') dailySummary[d].redMin += 0.5; // 30s sample = 0.5 min
    });
    const dailyStats = Object.entries(dailySummary).sort().map(([date, s]) => ({
      date,
      activeHours: Math.round(s.active * 0.5 / 60 * 10) / 10,
      peakKeys: Math.round(s.peakKeys),
      peakPrompts: s.peakPrompts,
      redMinutes: Math.round(s.redMin),
    }));

    const firstTs = rows[0]?.timestamp || 0;
    const lastTs = rows[rows.length - 1]?.timestamp || 0;

    // --- Energy Flow: phase classification per 10-min window ---
    // Phases derived from keys-vs-prompts ratio, not hardcoded times
    // deep_work: high keys, low prompts (building)
    // directing: low keys, high prompts (commanding roles)
    // dual_load: both elevated (transition risk — red predictor)
    // recovery: both low (break or idle)
    const WINDOW = 600; // 10 minutes
    const energyFlow: { timestamp: number; phase: string; keys: number; prompts: number; hour: number }[] = [];
    if (day.length > 0) {
      let wStart = day[0].timestamp;
      let kSum = 0, pSum = 0, cnt = 0;
      for (const r of day) {
        if (r.timestamp - wStart >= WINDOW) {
          const kAvg = cnt ? kSum / cnt : 0;
          const pAvg = cnt ? pSum / cnt : 0;
          const hr = new Date((wStart - 5 * 3600) * 1000).getUTCHours();
          let phase = 'recovery';
          if (kAvg > 15 && pAvg > 20) phase = 'dual_load';
          else if (kAvg > 15) phase = 'deep_work';
          else if (pAvg > 10) phase = 'directing';
          energyFlow.push({ timestamp: wStart, phase, keys: Math.round(kAvg), prompts: Math.round(pAvg), hour: hr });
          wStart = r.timestamp;
          kSum = 0; pSum = 0; cnt = 0;
        }
        kSum += r.keys_per_min;
        pSum += r.prompts_1h;
        cnt++;
      }
      if (cnt > 0) {
        const kAvg = kSum / cnt;
        const pAvg = pSum / cnt;
        const hr = new Date((wStart - 5 * 3600) * 1000).getUTCHours();
        let phase = 'recovery';
        if (kAvg > 15 && pAvg > 20) phase = 'dual_load';
        else if (kAvg > 15) phase = 'deep_work';
        else if (pAvg > 10) phase = 'directing';
        energyFlow.push({ timestamp: wStart, phase, keys: Math.round(kAvg), prompts: Math.round(pAvg), hour: hr });
      }
    }

    // Phase summary
    const phaseCounts = { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 };
    energyFlow.forEach(e => { if (phaseCounts[e.phase as keyof typeof phaseCounts] !== undefined) phaseCounts[e.phase as keyof typeof phaseCounts]++; });
    const phaseTotal = energyFlow.length || 1;
    const phasePcts = {
      deep_work: Math.round(phaseCounts.deep_work / phaseTotal * 100),
      directing: Math.round(phaseCounts.directing / phaseTotal * 100),
      dual_load: Math.round(phaseCounts.dual_load / phaseTotal * 100),
      recovery: Math.round(phaseCounts.recovery / phaseTotal * 100),
    };

    // Transition risk: consecutive dual_load windows
    let maxDualStreak = 0, curDualStreak = 0;
    const transitionRisks: { timestamp: number; duration: number }[] = [];
    let streakStart = 0;
    energyFlow.forEach(e => {
      if (e.phase === 'dual_load') {
        if (curDualStreak === 0) streakStart = e.timestamp;
        curDualStreak++;
      } else {
        if (curDualStreak >= 2) {
          transitionRisks.push({ timestamp: streakStart, duration: curDualStreak * 10 });
        }
        if (curDualStreak > maxDualStreak) maxDualStreak = curDualStreak;
        curDualStreak = 0;
      }
    });
    if (curDualStreak >= 2) {
      transitionRisks.push({ timestamp: streakStart, duration: curDualStreak * 10 });
    }
    if (curDualStreak > maxDualStreak) maxDualStreak = curDualStreak;

    // Break effectiveness: compare 30-min pre/post break windows
    const breakEffectiveness: { breakAt: number; preMeanIntensity: number; postMeanIntensity: number; effective: boolean }[] = [];
    // Find breaks: gaps in day data where since_last_min > 10
    for (let i = 1; i < day.length; i++) {
      const gap = day[i].timestamp - day[i - 1].timestamp;
      if (gap > 600) { // >10 min gap
        // Pre-break: last 60 samples before gap (~30 min)
        const preSlice = day.slice(Math.max(0, i - 60), i);
        const postSlice = day.slice(i, Math.min(day.length, i + 60));
        if (preSlice.length > 5 && postSlice.length > 5) {
          const preScore = preSlice.reduce((s, r) => s + r.keys_per_min + r.prompts_1h, 0) / preSlice.length;
          const postScore = postSlice.reduce((s, r) => s + r.keys_per_min + r.prompts_1h, 0) / postSlice.length;
          breakEffectiveness.push({
            breakAt: day[i - 1].timestamp,
            preMeanIntensity: Math.round(preScore),
            postMeanIntensity: Math.round(postScore),
            effective: postScore < preScore * 0.8, // 20%+ drop = effective
          });
        }
      }
    }

    // Role-phase alignment: which role gets prompts during which phase
    const rolePhaseMap: Record<string, Record<string, number>> = {
      wren: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
      silas: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
      kade: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
    };
    for (const role of ['wren', 'silas', 'kade']) {
      const logPath = path.join(PROMPT_DIR, `${role}-prompt-times.log`);
      if (fs.existsSync(logPath)) {
        const times = fs.readFileSync(logPath, 'utf-8').trim().split('\n')
          .map(t => parseInt(t, 10)).filter(t => !isNaN(t) && now - t < 86400);
        times.forEach(t => {
          // Find the energy flow window this prompt falls into
          for (let i = energyFlow.length - 1; i >= 0; i--) {
            if (t >= energyFlow[i].timestamp) {
              rolePhaseMap[role][energyFlow[i].phase]++;
              break;
            }
          }
        });
      }
    }

    const energyData = {
      flow: energyFlow,
      phasePcts,
      transitionRisks,
      maxDualStreakMin: maxDualStreak * 10,
      breakEffectiveness,
      rolePhaseAlignment: rolePhaseMap,
    };

    res.json({
      headline,
      hourOfDay,
      intensityTimeline: bands,
      breakPatterns,
      roleAttention,
      typingVsPrompting,
      dailyStats,
      energyFlow: energyData,
      current,
      meta: {
        rows: rows.length,
        from: new Date(firstTs * 1000).toISOString(),
        to: new Date(lastTs * 1000).toISOString(),
        daysSpanned: Math.round((lastTs - firstTs) / 86400 * 10) / 10,
      },
    });
  } catch (err) {
    res.status(500).json({ error: 'Failed to compute attention analytics', detail: String(err) });
  }
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

app.get('/api/chorus/perf', (_req: Request, res: Response) => {
  execFile('bash', [PERF_SCRIPT, 'summary'], { timeout: 30000 }, (err, stdout, stderr) => {
    if (err) {
      res.status(500).json({ error: 'perf-baseline.sh failed', detail: stderr.trim() });
      return;
    }
    // Parse tabular output: "Function  Today  Yesterday  Delta  Status"
    const lines = stdout.trim().split('\n');
    const headerLine = lines.findIndex(l => /^Function\s/.test(l));
    const dateLine = lines.find(l => /^Perf Baseline/.test(l));
    const summaryLine = lines.find(l => /passed/.test(l));

    const date = dateLine?.replace('Perf Baseline — ', '').trim() || null;
    const results: { function: string; today_ms: number; yesterday_ms: number; delta_pct: string; status: string }[] = [];

    if (headerLine >= 0) {
      for (let i = headerLine + 2; i < lines.length; i++) {
        const line = lines[i].trim();
        if (!line || /passed/.test(line)) continue;
        // Parse: "fuseki:graph_count  3,432ms  2,653ms  ▲+29% !  PASS"
        const match = line.match(/^(\S+)\s+([\d,]+)ms\s+([\d,]+)ms\s+(.+?)\s+(PASS|FAIL)\s*$/);
        if (match) {
          results.push({
            function: match[1],
            today_ms: parseInt(match[2].replace(/,/g, ''), 10),
            yesterday_ms: parseInt(match[3].replace(/,/g, ''), 10),
            delta_pct: match[4].trim(),
            status: match[5],
          });
        }
      }
    }

    const passed = results.filter(r => r.status === 'PASS').length;
    const total = results.length;

    res.json({
      date,
      summary: summaryLine?.trim() || `${passed}/${total} passed`,
      passed,
      total,
      results,
    });
  });
});

// --- GET /api/chorus/services — LaunchAgent service status (#1485) ---

app.get('/api/chorus/services', (_req: Request, res: Response) => {
  execFile('launchctl', ['list'], { timeout: 10000 }, (err, stdout) => {
    if (err) {
      res.status(500).json({ error: 'launchctl list failed' });
      return;
    }

    const services: { label: string; pid: number | null; status: number }[] = [];
    for (const line of stdout.trim().split('\n').slice(1)) {
      const parts = line.split('\t');
      if (parts.length < 3) continue;
      const label = parts[2];
      if (!label.startsWith('com.chorus.') && !label.startsWith('com.gathering.')) continue;
      services.push({
        label,
        pid: parts[0] === '-' ? null : parseInt(parts[0], 10),
        status: parseInt(parts[1], 10),
      });
    }

    // Get RSS for running services (by PID)
    const pids = services.filter(s => s.pid).map(s => s.pid!);
    if (pids.length === 0) {
      res.json({ services, running: 0, total: services.length });
      return;
    }

    execFile('ps', ['-o', 'pid=,rss=', '-p', pids.join(',')], { timeout: 5000 }, (psErr, psOut) => {
      const rssMap = new Map<number, number>();
      if (!psErr && psOut) {
        for (const line of psOut.trim().split('\n')) {
          const [pidStr, rssStr] = line.trim().split(/\s+/);
          if (pidStr && rssStr) {
            rssMap.set(parseInt(pidStr, 10), Math.round(parseInt(rssStr, 10) / 1024)); // KB → MB
          }
        }
      }

      const enriched = services.map(s => ({
        ...s,
        rss_mb: s.pid ? (rssMap.get(s.pid) || null) : null,
      }));

      const running = enriched.filter(s => s.pid !== null).length;
      const totalRss = enriched.reduce((sum, s) => sum + (s.rss_mb || 0), 0);

      res.json({
        services: enriched,
        running,
        total: enriched.length,
        total_rss_mb: totalRss,
      });
    });
  });
});

// --- GET /api/chorus/disk — Disk usage summary (#1485) ---

app.get('/api/chorus/disk', (_req: Request, res: Response) => {
  // Two data sources: diskutil for container total, osascript for Finder free (includes purgeable)
  execFile('/usr/sbin/diskutil', ['info', '/'], { timeout: 10000 }, (diskErr, diskStdout) => {
    if (diskErr) {
      res.status(500).json({ error: 'diskutil info failed', detail: diskErr.message });
      return;
    }

    const extract = (label: string): string | null => {
      const match = diskStdout.match(new RegExp(`${label}:\\s*(.+)`));
      return match ? match[1].trim() : null;
    };

    const totalSize = extract('Container Total Space');
    const containerFreeSize = extract('Container Free Space');

    const parseBytes = (s: string | null): number | null => {
      if (!s) return null;
      const m = s.match(/\((\d+)\s*Bytes\)/);
      return m ? parseInt(m[1], 10) : null;
    };

    const totalBytes = parseBytes(totalSize);
    const containerFreeBytes = parseBytes(containerFreeSize);

    // Finder free space includes purgeable — matches what Jeff sees in Finder
    execFile('/usr/bin/osascript', ['-e', 'tell application "Finder" to get free space of startup disk'],
      { timeout: 5000 }, (finderErr, finderStdout) => {
      const finderFreeBytes = finderErr ? null : Math.round(parseFloat(finderStdout.trim()));
      const freeBytes = finderFreeBytes ?? containerFreeBytes;
      const usedBytes = totalBytes && freeBytes ? totalBytes - freeBytes : null;
      const usedPct = totalBytes && usedBytes ? Math.round((usedBytes / totalBytes) * 100) : null;

      res.json({
        machine: 'Library',
        total: totalSize,
        free: containerFreeSize,
        total_bytes: totalBytes,
        container_free_bytes: containerFreeBytes,
        finder_free_bytes: finderFreeBytes,
        free_bytes: freeBytes,
        used_bytes: usedBytes,
        used_pct: usedPct,
        warning: usedPct !== null && usedPct >= 90,
        critical: usedPct !== null && usedPct >= 95,
      });
    });
  });
});

// --- GET /api/chorus/harvest — Harvest pipeline status (#1485) ---

const HARVEST_EXPORTER = path.join(process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus'), 'platform/scripts/harvest-exporter.sh');

app.get('/api/chorus/harvest', (_req: Request, res: Response) => {
  // Query Fuseki for graph counts per domain
  const sparql = `
    SELECT ?g (COUNT(*) AS ?count) WHERE {
      GRAPH ?g { ?s ?p ?o }
    } GROUP BY ?g ORDER BY DESC(?count)
  `;

  fetch(`${FUSEKI_URL}?query=${encodeURIComponent(sparql)}`, {
    headers: { 'Accept': 'application/sparql-results+json' },
    signal: AbortSignal.timeout(30000),
  })
    .then(r => { if (!r.ok) throw new Error(`Fuseki: ${r.status}`); return r.json(); })
    .then((data: any) => {
      const graphs: { name: string; triples: number }[] = [];
      let totalTriples = 0;
      for (const b of data.results.bindings) {
        const name = (b.g?.value || '').split('/').pop() || '';
        const count = parseInt(b.count?.value || '0', 10);
        graphs.push({ name, triples: count });
        totalTriples += count;
      }

      // Aggregate by domain prefix
      const domains: Record<string, { graphs: number; triples: number }> = {};
      for (const g of graphs) {
        // Extract domain from graph name patterns like "music-albums.ttl", "photos-2024.ttl"
        const domain = g.name.replace(/[-_].*$/, '').replace(/\.ttl$/, '').toLowerCase();
        if (!domains[domain]) domains[domain] = { graphs: 0, triples: 0 };
        domains[domain].graphs++;
        domains[domain].triples += g.triples;
      }

      res.json({
        total_graphs: graphs.length,
        total_triples: totalTriples,
        domains: Object.entries(domains)
          .sort((a, b) => b[1].triples - a[1].triples)
          .map(([name, d]) => ({ name, ...d })),
      });
    })
    .catch(err => {
      res.status(500).json({ error: 'Fuseki query failed', detail: String(err) });
    });
});

// --- GET /api/chorus/cost — Cost summary (#1485) ---

const COST_SCRIPT = path.join(process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus'), 'platform/scripts/cost-report.sh');

app.get('/api/chorus/cost', (req: Request, res: Response) => {
  const period = (req.query.period as string) || 'summary';
  execFile('bash', [COST_SCRIPT, period], { timeout: 15000, env: { ...process.env, HOME: os.homedir() } }, (err, stdout, stderr) => {
    // Cost script may fail due to missing data — still return what we got
    const output = (stdout || '').trim();
    const errors = (stderr || '').trim();
    if (!output && err) {
      res.status(500).json({ error: 'cost-report.sh failed', detail: errors });
      return;
    }
    res.json({
      period,
      output,
      partial: !!errors,
    });
  });
});

// --- Seeds endpoint (#1869) ---

app.get('/api/chorus/seeds', async (_req: Request, res: Response) => {
  try {
    const fusekiUrl = process.env.FUSEKI_URL || 'http://localhost:3030';
    const query = `
      PREFIX jb: <https://jeffbridwell.com/ontology#>
      SELECT DISTINCT ?slug ?content ?seedUrl ?linkTitle ?seededAt ?routedTo
      WHERE {
        GRAPH <urn:jb:seeds/> {
          ?s jb:slug ?slug .
          OPTIONAL { ?s jb:seedContent ?content }
          OPTIONAL { ?s jb:seedUrl ?seedUrl }
          OPTIONAL { ?s jb:linkTitle ?linkTitle }
          OPTIONAL { ?s jb:seededAt ?seededAt }
          OPTIONAL { ?s jb:routedTo ?routedTo }
        }
      }
      LIMIT 50
    `;
    const url = `${fusekiUrl}/pods/query`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/json' },
      body: `query=${encodeURIComponent(query)}`,
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) {
      res.status(502).json({ error: 'Fuseki query failed', status: response.status });
      return;
    }
    const data = await response.json() as { results: { bindings: Array<Record<string, { value: string }>> } };
    const seeds = data.results.bindings.map(b => ({
      slug: b.slug?.value,
      content: b.content?.value?.substring(0, 200),
      seedUrl: b.seedUrl?.value,
      linkTitle: b.linkTitle?.value,
      status: b.status?.value || 'pending',
      type: b.type?.value,
      source: b.source?.value,
      seededAt: b.seededAt?.value,
      routedTo: b.routedTo?.value,
    }));
    res.json({ seeds, total: seeds.length });
  } catch (err) {
    res.status(500).json({ error: 'Seeds query failed', detail: err instanceof Error ? err.message : String(err) });
  }
});

// --- Seed media serving (#2007) ---

const SEED_MEDIA_DIR = path.resolve(__dirname, '../../../../jeff-bridwell-personal-site/data/pods/jeff/capture/media');

app.get('/api/chorus/seed-media/:filename', (req: Request, res: Response) => {
  const filename = req.params.filename;
  // Validate: only alphanumeric, hyphens, dots — no path traversal
  if (!/^[a-zA-Z0-9._-]+$/.test(filename)) {
    res.status(400).json({ error: 'Invalid filename' });
    return;
  }
  const filePath = path.join(SEED_MEDIA_DIR, filename);
  if (!fs.existsSync(filePath)) {
    res.status(404).json({ error: 'Media not found' });
    return;
  }
  res.sendFile(filePath);
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
  infrastructure: { product: 'chorus', step: 'building', description: 'Servers, LaunchAgents, Docker, deploy, disk, network. Two machines.' },
  'knowledge-graph': { product: 'gathering', step: 'practicing', description: 'RDF/SPARQL semantic layer. Fuseki, ontologies, SHACL validation.' },
  observability: { product: 'chorus', step: 'building', description: 'Grafana, Loki, Promtail, alerts. Operational visibility.' },
  loom:      { product: 'chorus',    step: 'directing', description: 'Team coordination surface. Roles, cards, briefs, decisions.' },
};

app.get('/api/chorus/domain/:name', async (_req: Request, res: Response) => {
  const name = _req.params.name.toLowerCase();
  const meta = DOMAIN_REGISTRY[name];
  if (!meta) {
    res.status(404).json({ error: `Unknown domain: ${name}`, validDomains: Object.keys(DOMAIN_REGISTRY) });
    return;
  }

  try {
    const boardTs = `${CHORUS_ROOT}/platform/scripts/cards`;
    const envOpts = {
      encoding: 'utf-8' as const, timeout: 15000,
      env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
    };

    let cards: { id: string; title: string; status: string; owner: string; type: string }[] = [];
    try {
      const { stdout } = await execAsync(`bash ${boardTs} list 2>/dev/null`, envOpts);
      const output = stdout.trim();
      let currentStatus = '';
      for (const line of output.split('\n')) {
        const statusMatch = line.match(/^(WIP|Blocked|Next|Later|Done|Won't Do)\s*\(\d+\)/);
        if (statusMatch) { currentStatus = statusMatch[1]; continue; }
        if (currentStatus === 'Done' || currentStatus === "Won't Do") continue;
        const cardMatch = line.trim().match(/^(\d+)\s+(.+?)\s+\[([^\]]+)\]$/);
        if (cardMatch) {
          const tags = cardMatch[3];
          if (!tags.includes(`domain:${name}`)) continue;
          const ownerMatch = tags.match(/^(Wren|Silas|Kade)/i);
          const typeMatch = tags.match(/type:(\w+)/);
          cards.push({
            id: cardMatch[1],
            title: cardMatch[2].trim(),
            status: currentStatus,
            owner: ownerMatch ? ownerMatch[1].toLowerCase() : '',
            type: typeMatch ? typeMatch[1] : '',
          });
        }
      }
    } catch {}

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
    try {
      const sdId = `${name}-service`;
      const cRes = await fetch(`http://localhost:3340/api/athena/subdomains/${sdId}/completeness`);
      if (cRes.ok) {
        const cBody = await cRes.json() as any;
        completeness = cBody.data;
      } else {
        // Try domain suffix
        const cRes2 = await fetch(`http://localhost:3340/api/athena/subdomains/${name}-domain/completeness`);
        if (cRes2.ok) {
          const cBody2 = await cRes2.json() as any;
          completeness = cBody2.data;
        }
      }
    } catch {}

    if (completeness && completeness.percentage < 60) {
      console.warn(`[domain-completeness] ${name}: ${completeness.percentage}% — missing: ${completeness.missing?.join(', ')}`);
    }

    res.json({
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
    });
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

setTimeout(() => refreshHealthCache(), 2000);
setInterval(() => refreshHealthCache(), 30_000);

// Scheduled reindex — keep index_freshness sources current (#1960)
// indexAllSources() is SQLite-only (no Ollama), safe in-process unlike embedDelta (#1978).
// Runs every 15 min. First run after 60s startup delay to avoid boot contention.
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
setTimeout(() => {
  scheduledReindex();
  setInterval(() => scheduledReindex(), REINDEX_INTERVAL);
}, 60_000);

// SHACL validation — check ontology integrity (#2014)
app.get('/api/athena/validate', async (_req: Request, res: Response) => {
  const start = Date.now();
  const FUSEKI = 'http://localhost:3030';
  const violations: Array<{ node: string; constraint: string; severity: string; message: string }> = [];
  const warnings: Array<{ node: string; constraint: string; severity: string; message: string }> = [];

  const checks = [
    {
      name: 'Product must have Domain',
      severity: 'violation',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:Product . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasDomain ?d }
        }}`,
    },
    {
      name: 'Product must have ServiceDesign',
      severity: 'violation',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:Product . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasServiceDesign ?sd }
        }}`,
    },
    {
      name: 'SubProduct must have parent Product',
      severity: 'violation',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubProduct . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?parent chorus:hasSubProduct ?node }
        }}`,
    },
    {
      name: 'SubProduct must have SubDomain',
      severity: 'violation',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubProduct . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasDomain ?d }
        }}`,
    },
    {
      name: 'SubDomain must have parent',
      severity: 'violation',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubDomain . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?parent chorus:hasDomain ?node }
        }}`,
    },
    {
      name: 'SubDomain has no instances (incomplete)',
      severity: 'warning',
      query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubDomain . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:contains ?i }
        }}`,
    },
  ];

  try {
    for (const check of checks) {
      const result = await fetch(`${FUSEKI}/pods/query?query=${encodeURIComponent(check.query)}`, {
        headers: { Accept: 'application/json' },
        signal: AbortSignal.timeout(10000),
      });
      if (!result.ok) continue;
      const data = await result.json() as { results: { bindings: Array<{ node: { value: string }; label?: { value: string } }> } };
      for (const b of data.results.bindings) {
        const entry = {
          node: b.label?.value || b.node.value.replace('https://jeffbridwell.com/chorus#', ''),
          constraint: check.name,
          severity: check.severity,
          message: check.name,
        };
        if (check.severity === 'violation') violations.push(entry);
        else warnings.push(entry);
      }
    }
    res.json({
      valid: violations.length === 0,
      violations,
      warnings,
      checked: checks.length,
      duration_ms: Date.now() - start,
      timestamp: bostonNow(),
    });
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('validate', { error: err.message }, Date.now() - start, { error: true }));
  }
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

app.get('/api/chorus/hooks/metrics', (_req: Request, res: Response) => {
  if (hooksMetricsCache && (Date.now() - hooksMetricsCache.ts) < HOOKS_CACHE_TTL) {
    res.json(hooksMetricsCache.data);
    return;
  }

  const HOOKS_LOG = path.join(os.homedir(), 'Library/Logs/Gathering/hooks.log');

  if (!fs.existsSync(HOOKS_LOG)) {
    res.status(503).json({ error: 'hooks.log not found' });
    return;
  }

  try {
    const raw = fs.readFileSync(HOOKS_LOG, 'utf-8');
    const lines = raw.trim().split('\n');

    // Filter to last 7 days
    const cutoff = new Date();
    cutoff.setDate(cutoff.getDate() - 7);
    const cutoffStr = cutoff.toISOString().slice(0, 10);

    const modules: Record<string, { allow: number; deny: number; warn: number; total: number }> = {};
    let totalDecisions = 0;

    for (const line of lines) {
      // Format: timestamp | hook_type | tool | role | module | decision | duration | session_id | context
      const parts = line.split('|').map(s => s.trim());
      if (parts.length < 6) continue;

      const timestamp = parts[0].slice(0, 10);
      if (timestamp < cutoffStr) continue;

      const moduleName = parts[4];
      const decision = parts[5].toLowerCase();

      // Skip internal entries (enter, no module)
      if (!moduleName || moduleName === '-' || moduleName === 'none' || decision === 'enter') continue;

      if (!modules[moduleName]) {
        modules[moduleName] = { allow: 0, deny: 0, warn: 0, total: 0 };
      }

      modules[moduleName].total++;
      totalDecisions++;

      if (decision === 'allow') modules[moduleName].allow++;
      else if (decision === 'deny' || decision === 'block') modules[moduleName].deny++;
      else if (decision === 'warn') modules[moduleName].warn++;
    }

    // Enforced = modules with at least one deny
    const enforcedModules = Object.keys(modules).filter(m => modules[m].deny > 0).length;
    const totalModules = Object.keys(modules).length;
    const enforcementPercent = totalModules > 0 ? Math.round((enforcedModules / totalModules) * 100) : 0;

    const result = {
      totalDecisions,
      totalModules,
      enforcedModules,
      enforcementPercent,
      periodDays: 7,
      modules,
      generatedAt: new Date().toISOString(),
    };
    hooksMetricsCache = { data: result, ts: Date.now() };
    res.json(result);
  } catch (err) {
    res.status(500).json({ error: 'Failed to parse hooks log' });
  }
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
app.get('/api/athena/health', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('health'));
    const count = parseInt(result.results.bindings[0]?.count?.value || '0', 10);
    res.json(athenaEnvelope('health', { status: 'ok', tripleCount: count, endpoint: ATHENA_SPARQL, queries: ATHENA_QUERIES }, Date.now() - start));
  } catch (err: any) {
    res.status(503).json(athenaEnvelope('health', { status: 'error', message: err.message, queries: ATHENA_QUERIES }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/products — list all products
app.get('/api/athena/products', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('products'));
    const products = result.results.bindings.map((b: any) => ({
      uri: b.product.value,
      label: b.label?.value || b.product.value.split('#').pop(),
    }));
    res.json(athenaEnvelope('products', products, Date.now() - start, { count: products.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('products', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subproducts — list sub-products with owner, domain count, consumes count
app.get('/api/athena/subproducts', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('subproducts'));
    const subproducts = result.results.bindings.map((b: any) => ({
      uri: b.sp.value,
      label: b.label?.value || b.sp.value.split('#').pop(),
      owner: b.ownerLabel?.value || null,
      domainCount: parseInt(b.domainCount?.value || '0', 10),
      consumesCount: parseInt(b.consumesCount?.value || '0', 10),
    }));
    res.json(athenaEnvelope('subproducts', subproducts, Date.now() - start, { count: subproducts.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subproducts', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains — list sub-domains with owner, step. Filter: ?owner, ?step
app.get('/api/athena/subdomains', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    let query = loadSparql('subdomains');
    const filters: string[] = [];
    if (req.query.owner) filters.push(`FILTER(LCASE(STR(?ownerLabel)) = "${String(req.query.owner).toLowerCase()}")`);
    if (req.query.step) filters.push(`FILTER(LCASE(STR(?stepLabel)) = "${String(req.query.step).toLowerCase()}")`);
    if (filters.length) query = query.replace('} ORDER BY', `${filters.join('\n  ')}\n} ORDER BY`);
    const result = await athenaSparqlQuery(query);
    const subdomains = result.results.bindings.map((b: any) => ({
      uri: b.sd.value,
      id: b.sd.value.split('#').pop(),
      label: b.label?.value || b.sd.value.split('#').pop(),
      owner: b.ownerLabel?.value || null,
      step: b.stepLabel?.value || null,
    }));
    res.json(athenaEnvelope('subdomains', subdomains, Date.now() - start, { count: subdomains.length, filters: { owner: req.query.owner || null, step: req.query.step || null } }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomains', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/blast-radius — what breaks if this sub-domain fails
app.get('/api/athena/subdomains/:id/blast-radius', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = loadSparql('blast-radius').replace('$URI', sdUri);
    const result = await athenaSparqlQuery(query);
    const consumers = result.results.bindings.map((b: any) => ({
      uri: b.consumer.value,
      label: b.consumerLabel?.value || b.consumer.value.split('#').pop(),
    }));
    res.json(athenaEnvelope('blast-radius', { subdomain: req.params.id, consumers }, Date.now() - start, { count: consumers.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('blast-radius', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id — single sub-domain detail
app.get('/api/athena/subdomains/:id', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = loadSparql('subdomain-detail').split('$URI').join(sdUri);
    const result = await athenaSparqlQuery(query);
    const bindings = result.results.bindings;
    if (bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-detail', {
        error: `Sub-domain '${req.params.id}' not found`,
        suggestion: 'Use GET /api/athena/subdomains to list all available sub-domains.',
      }, Date.now() - start, { error: true }));
    }
    const first = bindings[0];
    const consumers = [...new Set(bindings.filter((b: any) => b.consumer).map((b: any) => JSON.stringify({ uri: b.consumer.value, label: b.consumerLabel?.value || b.consumer.value.split('#').pop() })))].map((s: any) => JSON.parse(s));
    const consumes = [...new Set(bindings.filter((b: any) => b.consumed).map((b: any) => JSON.stringify({ uri: b.consumed.value, label: b.consumedLabel?.value || b.consumed.value.split('#').pop() })))].map((s: any) => JSON.parse(s));
    const domains = [...new Set(bindings.filter((b: any) => b.child).map((b: any) => JSON.stringify({ uri: b.child.value, id: b.child.value.split('#').pop(), label: b.childLabel?.value || b.child.value.split('#').pop() })))].map((s: any) => JSON.parse(s));
    const instanceMap = new Map<string, { uri: string; id: string; label: string; comment: string | null; type: string | null }>();
    for (const b of bindings) {
      if (!b.instance) continue;
      const uri = b.instance.value;
      if (!instanceMap.has(uri)) {
        instanceMap.set(uri, {
          uri,
          id: uri.split('#').pop() || '',
          label: b.instanceLabel?.value || uri.split('#').pop() || '',
          comment: b.instanceComment?.value || null,
          type: b.instanceTypeLabel?.value || b.instanceType?.value?.split('#').pop() || null,
        });
      }
    }
    const instances = [...instanceMap.values()];
    res.json(athenaEnvelope('subdomain-detail', {
      uri: sdUri,
      id: req.params.id,
      label: first.label?.value || req.params.id,
      owner: first.ownerLabel?.value || null,
      step: first.stepLabel?.value || null,
      comment: first.comment?.value || null,
      consumedBy: consumers,
      consumes,
      domains,
      instances,
    }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-detail', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/steps — value stream steps with sub-domains at each step
app.get('/api/athena/steps', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('steps'));
    const stepMap = new Map<string, { uri: string; label: string; domainCount: number; subdomains: { uri: string; label: string; owner: string | null }[] }>();
    for (const b of result.results.bindings) {
      const key = b.step.value;
      if (!stepMap.has(key)) {
        stepMap.set(key, { uri: key, label: b.stepLabel?.value || key.split('#').pop()!, domainCount: 0, subdomains: [] });
      }
      if (b.sd) {
        const entry = stepMap.get(key)!;
        entry.subdomains.push({ uri: b.sd.value, label: b.sdLabel?.value || b.sd.value.split('#').pop()!, owner: b.sdOwnerLabel?.value || null });
        entry.domainCount = entry.subdomains.length;
      }
    }
    const steps = Array.from(stepMap.values());
    res.json(athenaEnvelope('steps', steps, Date.now() - start, { count: steps.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('steps', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/owners — owners with sub-domain counts
app.get('/api/athena/owners', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('owners'));
    const owners = result.results.bindings.map((b: any) => ({
      uri: b.owner.value,
      label: b.label?.value || b.owner.value.split('#').pop(),
      subdomainCount: parseInt(b.count.value, 10),
    }));
    res.json(athenaEnvelope('owners', owners, Date.now() - start, { count: owners.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('owners', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/machines — machines with running services
app.get('/api/athena/machines', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const result = await athenaSparqlQuery(loadSparql('machines'));
    const machineMap = new Map<string, { uri: string; label: string; ip: string | null; role: string | null; services: { uri: string; label: string }[] }>();
    for (const b of result.results.bindings) {
      const uri = b.machine.value;
      if (!machineMap.has(uri)) {
        machineMap.set(uri, {
          uri,
          label: b.label?.value || uri.split('#').pop(),
          ip: b.ip?.value || null,
          role: b.role?.value || null,
          services: [],
        });
      }
      if (b.service) {
        machineMap.get(uri)!.services.push({
          uri: b.service.value,
          label: b.serviceLabel?.value || b.service.value.split('#').pop(),
        });
      }
    }
    const machines = Array.from(machineMap.values());
    res.json(athenaEnvelope('machines', machines, Date.now() - start, { count: machines.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('machines', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/cards — active board cards for this domain
app.get('/api/athena/subdomains/:id/cards', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    // Map subdomain ID to board search terms
    const domainLabel = req.params.id.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
    // Board uses domain: and sequence: labels — match either
    const { execSync } = require('child_process');
    const CARDS_CLI = path.join(REPO_SCRIPTS, 'cards');
    const raw = execSync(`bash ${CARDS_CLI} list 2>/dev/null`, { encoding: 'utf-8', timeout: 10000 });
    // Search all lanes — Jeff wants every card for the domain visible (#1931)
    // Build ID→status map from full board output
    const statusMap = new Map<string, string>();
    let currentStatus = '';
    for (const line of raw.split('\n')) {
      const headerMatch = line.match(/^(WIP|Now|Next|Later|Done) /);
      if (headerMatch) { currentStatus = headerMatch[1]; continue; }
      const idMatch = line.match(/^\s+(\d+)\s/);
      if (idMatch && currentStatus) statusMap.set(idMatch[1], currentStatus);
    }
    const lines = raw.split('\n').filter((l: string) => {
      if (!l.match(/^\s+\d+/)) return false;
      return l.includes(`domain:${domainLabel}`) || l.includes(`sequence:${domainLabel}`);
    });
    const cards = lines.map((l: string) => {
      const match = l.match(/^\s+(\d+)\s+(.+?)\s+\[([^\]]*)\]/);
      if (!match) return null;
      const [, id, title, meta] = match;
      const owner = meta.match(/^(Wren|Silas|Kade|Jeff)/i)?.[1]?.toLowerCase() || null;
      const priority = meta.match(/P([1-3])/)?.[0] || null;
      return { id, title: title.replace(/ — /g, ' — ').trim(), owner, status: statusMap.get(id!) || '', priority };
    }).filter(Boolean);
    res.json(athenaEnvelope('subdomain-cards', { subdomain: req.params.id, domainLabel, cards }, Date.now() - start, { count: cards.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-cards', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/alerts — alert rules related to this domain
app.get('/api/athena/subdomains/:id/alerts', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const domainLabel = req.params.id.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
    const ALERTS_DIR = path.join(REPO_ROOT, 'proving/domains/alerts');
    const yaml = require('js-yaml') || null;
    const alertFiles = fs.readdirSync(ALERTS_DIR).filter((f: string) => f.endsWith('.yml'));
    const alerts: any[] = [];
    for (const file of alertFiles) {
      const content = fs.readFileSync(path.join(ALERTS_DIR, file), 'utf-8');
      // Match by domain keyword in filename, name, description, or check script
      const lower = content.toLowerCase();
      if (lower.includes(domainLabel) || file.toLowerCase().includes(domainLabel)) {
        // Parse basic fields from YAML comments and keys
        const name = content.match(/^name:\s*(.+)/m)?.[1]?.trim() || file.replace('.yml', '');
        const description = content.match(/^description:\s*(.+)/m)?.[1]?.trim() || '';
        const severity = content.match(/^severity:\s*(.+)/m)?.[1]?.trim() || 'unknown';
        const schedule = content.match(/^schedule:\s*"?(.+?)"?\s*$/m)?.[1]?.trim() || '';
        alerts.push({ file, name, description, severity, schedule });
      }
    }
    res.json(athenaEnvelope('subdomain-alerts', { subdomain: req.params.id, domainLabel, alerts }, Date.now() - start, { count: alerts.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-alerts', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/code — code inventory from instances graph (#1868)
app.get('/api/athena/subdomains/:id/code', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?file ?label ?filePath ?fileType ?description WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasCodeFile ?file . OPTIONAL { ?file rdfs:label ?label } OPTIONAL { ?file chorus:filePath ?filePath } OPTIONAL { ?file chorus:fileType ?fileType } OPTIONAL { ?file rdfs:comment ?description } } }`;
    const result = await athenaSparqlQuery(query);
    const allFiles = result.results.bindings.map((b: any) => ({
      path: b.filePath?.value || b.label?.value || b.file.value.split('#').pop(),
      type: b.fileType?.value || path.extname(b.filePath?.value || '').slice(1) || 'unknown',
      description: b.description?.value || null,
    }));
    const isTest = (p: string) => /\/(tests?|__tests__)\//i.test(p) || /\.(test|spec)\./i.test(p) || /\.bats$/i.test(p) || /_test\.rs$/i.test(p) || /\.feature$/i.test(p);
    const tests = allFiles.filter((f: any) => isTest(f.path));
    const source = allFiles.filter((f: any) => !isTest(f.path));
    const byType = allFiles.reduce((acc: Record<string, number>, f: any) => { acc[f.type] = (acc[f.type] || 0) + 1; return acc; }, {});
    res.json(athenaEnvelope('subdomain-code', { subdomain: req.params.id, files: source, tests, byType }, Date.now() - start, { count: allFiles.length, source_count: source.length, test_count: tests.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-code', { error: err.message }, Date.now() - start, { error: true }));
  }
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
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?testFile ?testType WHERE { GRAPH <urn:chorus:instances> { ?tc a chorus:TestCoverage ; chorus:testFile ?testFile ; chorus:testType ?testType ; chorus:covers <${sdUri}> . } } ORDER BY ?testType ?testFile`;
    const result = await athenaSparqlQuery(query);
    const coverage = result.results.bindings.map((b: any) => ({
      testFile: b.testFile.value,
      testType: b.testType.value,
      coversDomain: req.params.id,
    }));
    res.json(athenaEnvelope('subdomain-coverage', { subdomain: req.params.id, coverage }, Date.now() - start, { count: coverage.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-coverage', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/test-coverage — what tests cover this domain? (#1869)
app.get('/api/athena/subdomains/:id/test-coverage', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?testFile ?testType WHERE { GRAPH <urn:chorus:instances> { ?tc a chorus:TestCoverage ; chorus:testFile ?testFile ; chorus:testType ?testType ; chorus:covers <${sdUri}> . } } ORDER BY ?testType ?testFile`;
    const result = await athenaSparqlQuery(query);
    const tests = result.results.bindings.map((b: any) => ({
      path: b.testFile.value,
      type: b.testType.value,
    }));
    const byType: Record<string, number> = {};
    for (const t of tests) { byType[t.type] = (byType[t.type] || 0) + 1; }
    res.json(athenaEnvelope('subdomain-test-coverage', { subdomain: req.params.id, tests, byType }, Date.now() - start, { count: tests.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-test-coverage', { error: err.message }, Date.now() - start, { error: true }));
  }
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
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?route ?filePath ?pageType WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasPage ?page . ?page a chorus:Page ; chorus:route ?route ; chorus:filePath ?filePath ; chorus:pageType ?pageType . } } ORDER BY ?pageType ?route`;
    const result = await athenaSparqlQuery(query);
    const pages = result.results.bindings.map((b: any) => ({
      route: b.route.value,
      path: b.filePath.value,
      pageType: b.pageType.value,
    }));
    const byType: Record<string, number> = {};
    for (const p of pages) { byType[p.pageType] = (byType[p.pageType] || 0) + 1; }
    res.json(athenaEnvelope('subdomain-pages', { subdomain: req.params.id, pages, byType }, Date.now() - start, { count: pages.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-pages', { error: err.message }, Date.now() - start, { error: true }));
  }
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
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?method ?routePath ?filePath WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasEndpoint ?ep . ?ep a chorus:Endpoint ; chorus:httpMethod ?method ; chorus:routePath ?routePath ; chorus:filePath ?filePath . } } ORDER BY ?method ?routePath`;
    const result = await athenaSparqlQuery(query);
    const endpoints = result.results.bindings.map((b: any) => ({
      method: b.method.value,
      path: b.routePath.value,
      handler: b.filePath.value,
    }));
    const byMethod: Record<string, number> = {};
    for (const e of endpoints) { byMethod[e.method] = (byMethod[e.method] || 0) + 1; }
    res.json(athenaEnvelope('subdomain-services', { subdomain: req.params.id, endpoints, byMethod }, Date.now() - start, { count: endpoints.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-services', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/actors — actors that interact with this subdomain (#1899)
app.get('/api/athena/subdomains/:id/actors', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?actor ?label ?role ?action WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasActor ?actor .
          OPTIONAL { ?actor rdfs:label ?label }
          OPTIONAL { ?actor chorus:actorRole ?role }
          OPTIONAL { ?actor chorus:actorAction ?action }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-actors', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const actors = result.results.bindings.map((b: any) => ({
      uri: b.actor.value,
      label: b.label?.value || b.actor.value.split('#').pop(),
      role: b.role?.value,
      action: b.action?.value,
    }));
    res.json(athenaEnvelope('subdomain-actors', { subdomain: req.params.id, actors }, Date.now() - start, { count: actors.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-actors', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/scenarios — BDD scenarios for this subdomain (#1899)
app.get('/api/athena/subdomains/:id/scenarios', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?scenario ?label ?given ?when ?then ?notes WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasScenario ?scenario .
          OPTIONAL { ?scenario rdfs:label ?label }
          OPTIONAL { ?scenario chorus:scenarioGiven ?given }
          OPTIONAL { ?scenario chorus:scenarioWhen ?when }
          OPTIONAL { ?scenario chorus:scenarioThen ?then }
          OPTIONAL { ?scenario chorus:scenarioNotes ?notes }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-scenarios', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const scenarios = result.results.bindings.map((b: any) => ({
      uri: b.scenario.value,
      label: b.label?.value || b.scenario.value.split('#').pop(),
      given: b.given?.value || null,
      when: b.when?.value || null,
      then: b.then?.value || null,
      notes: b.notes?.value || null,
    }));
    res.json(athenaEnvelope('subdomain-scenarios', { subdomain: req.params.id, scenarios }, Date.now() - start, { count: scenarios.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-scenarios', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/contract — API contract for this subdomain (#1899)
app.get('/api/athena/subdomains/:id/contract', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?contract ?label ?endpoint ?method ?description WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasContract ?contract .
          OPTIONAL { ?contract rdfs:label ?label }
          OPTIONAL { ?contract chorus:endpoint ?endpoint }
          OPTIONAL { ?contract chorus:httpMethod ?method }
          OPTIONAL { ?contract chorus:contractDescription ?description }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-contract', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const endpoints = result.results.bindings.map((b: any) => ({
      uri: b.contract.value,
      label: b.label?.value || b.contract.value.split('#').pop(),
      path: b.endpoint?.value || null,
      method: b.method?.value || null,
      description: b.description?.value || null,
    }));
    res.json(athenaEnvelope('subdomain-contract', { subdomain: req.params.id, endpoints }, Date.now() - start, { count: endpoints.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-contract', { error: err.message }, Date.now() - start, { error: true }));
  }
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

// GET /api/athena/subdomains/:id/pages — UI pages for this subdomain (#1923)
app.get('/api/athena/subdomains/:id/pages', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?page ?label ?route ?description ?status WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasPage ?page .
          OPTIONAL { ?page rdfs:label ?label }
          OPTIONAL { ?page chorus:pageRoute ?route }
          OPTIONAL { ?page chorus:pageDescription ?description }
          OPTIONAL { ?page chorus:pageStatus ?status }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-pages', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const pages = result.results.bindings.map((b: any) => ({
      uri: b.page.value,
      label: b.label?.value || b.page.value.split('#').pop(),
      route: b.route?.value || null,
      description: b.description?.value || null,
      status: b.status?.value || null,
    }));
    res.json(athenaEnvelope('subdomain-pages', { subdomain: req.params.id, pages }, Date.now() - start, { count: pages.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-pages', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/pages — add page to subdomain (#1923)
app.post('/api/athena/subdomains/:id/pages', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, route, description, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-page-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const pageId = `${req.params.id}-page-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const pageUri = `https://jeffbridwell.com/chorus#${pageId}`;
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${pageUri}> a chorus:Page ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasPage <${pageUri}> .
          ${route ? `<${pageUri}> chorus:pageRoute "${route.replace(/"/g, '\\"')}" .` : ''}
          ${description ? `<${pageUri}> chorus:pageDescription "${description.replace(/"/g, '\\"')}" .` : ''}
          ${status ? `<${pageUri}> chorus:pageStatus "${status.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-page-create', { subdomain: req.params.id, uri: pageUri, label, route: route || null, description: description || null, status: status || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-page-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/integrations — data integrations for this subdomain (#1923)
app.get('/api/athena/subdomains/:id/integrations', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?integration ?label ?source ?path ?status WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasIntegration ?integration .
          OPTIONAL { ?integration rdfs:label ?label }
          OPTIONAL { ?integration chorus:integrationSource ?source }
          OPTIONAL { ?integration chorus:integrationPath ?path }
          OPTIONAL { ?integration chorus:integrationStatus ?status }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-integrations', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const integrations = result.results.bindings.map((b: any) => ({
      uri: b.integration.value,
      label: b.label?.value || b.integration.value.split('#').pop(),
      source: b.source?.value || null,
      path: b.path?.value || null,
      status: b.status?.value || null,
    }));
    res.json(athenaEnvelope('subdomain-integrations', { subdomain: req.params.id, integrations }, Date.now() - start, { count: integrations.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-integrations', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/integrations — add integration to subdomain (#1923)
app.post('/api/athena/subdomains/:id/integrations', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, source, path: dataPath, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-integration-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const intId = `${req.params.id}-integration-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const intUri = `https://jeffbridwell.com/chorus#${intId}`;
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${intUri}> a chorus:Integration ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasIntegration <${intUri}> .
          ${source ? `<${intUri}> chorus:integrationSource "${source.replace(/"/g, '\\"')}" .` : ''}
          ${dataPath ? `<${intUri}> chorus:integrationPath "${dataPath.replace(/"/g, '\\"')}" .` : ''}
          ${status ? `<${intUri}> chorus:integrationStatus "${status.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-integration-create', { subdomain: req.params.id, uri: intUri, label, source: source || null, path: dataPath || null, status: status || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-integration-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/persistence — persistence stores for this subdomain (#1923)
app.get('/api/athena/subdomains/:id/persistence', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?store ?label ?type ?namespace ?records ?status WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasPersistence ?store .
          OPTIONAL { ?store rdfs:label ?label }
          OPTIONAL { ?store chorus:storeType ?type }
          OPTIONAL { ?store chorus:storeNamespace ?namespace }
          OPTIONAL { ?store chorus:storeRecordCount ?records }
          OPTIONAL { ?store chorus:storeStatus ?status }
        }
      }
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-persistence', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const stores = result.results.bindings.map((b: any) => ({
      uri: b.store.value,
      label: b.label?.value || b.store.value.split('#').pop(),
      type: b.type?.value || null,
      namespace: b.namespace?.value || null,
      records: b.records?.value ? parseInt(b.records.value) : null,
      status: b.status?.value || null,
    }));
    res.json(athenaEnvelope('subdomain-persistence', { subdomain: req.params.id, stores }, Date.now() - start, { count: stores.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-persistence', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/persistence — add persistence store to subdomain (#1923)
app.post('/api/athena/subdomains/:id/persistence', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, namespace, records, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-persistence-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const storeId = `${req.params.id}-store-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const storeUri = `https://jeffbridwell.com/chorus#${storeId}`;
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${storeUri}> a chorus:PersistenceStore ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasPersistence <${storeUri}> .
          ${type ? `<${storeUri}> chorus:storeType "${type.replace(/"/g, '\\"')}" .` : ''}
          ${namespace ? `<${storeUri}> chorus:storeNamespace "${namespace.replace(/"/g, '\\"')}" .` : ''}
          ${records != null ? `<${storeUri}> chorus:storeRecordCount "${records}" .` : ''}
          ${status ? `<${storeUri}> chorus:storeStatus "${status.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-persistence-create', { subdomain: req.params.id, uri: storeUri, label, type: type || null, namespace: namespace || null, records: records != null ? parseInt(records) : null, status: status || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-persistence-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/services — runtime services for this subdomain (#1924)
app.get('/api/athena/subdomains/:id/services', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?svc ?label ?type ?host ?status ?health WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasService ?svc . OPTIONAL { ?svc rdfs:label ?label } OPTIONAL { ?svc chorus:serviceType ?type } OPTIONAL { ?svc chorus:serviceHost ?host } OPTIONAL { ?svc chorus:serviceStatus ?status } OPTIONAL { ?svc chorus:healthEndpoint ?health } } }`;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) return res.status(404).json(athenaEnvelope('subdomain-services', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    const result = await athenaSparqlQuery(query);
    const services = result.results.bindings.map((b: any) => ({ uri: b.svc.value, label: b.label?.value || b.svc.value.split('#').pop(), type: b.type?.value || null, host: b.host?.value || null, status: b.status?.value || null, health_endpoint: b.health?.value || null }));
    res.json(athenaEnvelope('subdomain-services', { subdomain: req.params.id, services }, Date.now() - start, { count: services.length }));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-services', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/subdomains/:id/services — add service to subdomain (#1924)
app.post('/api/athena/subdomains/:id/services', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, host, status, health_endpoint } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-service-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const svcId = `${req.params.id}-service-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const svcUri = `https://jeffbridwell.com/chorus#${svcId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${svcUri}> a chorus:Service ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasService <${svcUri}> . ${type ? `<${svcUri}> chorus:serviceType "${type.replace(/"/g, '\\"')}" .` : ''} ${host ? `<${svcUri}> chorus:serviceHost "${host.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${svcUri}> chorus:serviceStatus "${status.replace(/"/g, '\\"')}" .` : ''} ${health_endpoint ? `<${svcUri}> chorus:healthEndpoint "${health_endpoint.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-service-create', { subdomain: req.params.id, uri: svcUri, label, type: type || null, host: host || null, status: status || null, health_endpoint: health_endpoint || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-service-create', { error: err.message }, Date.now() - start, { error: true })); }
});

// GET /api/athena/subdomains/:id/pipeline — data pipeline for this subdomain (#1925)
app.get('/api/athena/subdomains/:id/pipeline', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?pipe ?label ?source ?harvester ?icd ?status ?lastRun WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasPipeline ?pipe . OPTIONAL { ?pipe rdfs:label ?label } OPTIONAL { ?pipe chorus:pipelineSource ?source } OPTIONAL { ?pipe chorus:pipelineHarvester ?harvester } OPTIONAL { ?pipe chorus:pipelineICD ?icd } OPTIONAL { ?pipe chorus:pipelineStatus ?status } OPTIONAL { ?pipe chorus:pipelineLastRun ?lastRun } } }`;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) return res.status(404).json(athenaEnvelope('subdomain-pipeline', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    const result = await athenaSparqlQuery(query);
    const pipelines = result.results.bindings.map((b: any) => ({ uri: b.pipe.value, label: b.label?.value || b.pipe.value.split('#').pop(), source: b.source?.value || null, harvester: b.harvester?.value || null, icd: b.icd?.value || null, status: b.status?.value || null, last_run: b.lastRun?.value || null }));
    res.json(athenaEnvelope('subdomain-pipeline', { subdomain: req.params.id, pipelines }, Date.now() - start, { count: pipelines.length }));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-pipeline', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/subdomains/:id/pipeline — add pipeline to subdomain (#1925)
app.post('/api/athena/subdomains/:id/pipeline', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, source, harvester, icd, status, last_run } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-pipeline-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const pipeId = `${req.params.id}-pipeline-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const pipeUri = `https://jeffbridwell.com/chorus#${pipeId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${pipeUri}> a chorus:Pipeline ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPipeline <${pipeUri}> . ${source ? `<${pipeUri}> chorus:pipelineSource "${source.replace(/"/g, '\\"')}" .` : ''} ${harvester ? `<${pipeUri}> chorus:pipelineHarvester "${harvester.replace(/"/g, '\\"')}" .` : ''} ${icd ? `<${pipeUri}> chorus:pipelineICD "${icd.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${pipeUri}> chorus:pipelineStatus "${status.replace(/"/g, '\\"')}" .` : ''} ${last_run ? `<${pipeUri}> chorus:pipelineLastRun "${last_run}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-pipeline-create', { subdomain: req.params.id, uri: pipeUri, label, source: source || null, harvester: harvester || null, icd: icd || null, status: status || null, last_run: last_run || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-pipeline-create', { error: err.message }, Date.now() - start, { error: true })); }
});

// GET /api/athena/subdomains/:id/logs — log sources for this subdomain (#1926)
app.get('/api/athena/subdomains/:id/logs', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?log ?label ?location ?retention ?status WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasLogSource ?log . OPTIONAL { ?log rdfs:label ?label } OPTIONAL { ?log chorus:logSourceLocation ?location } OPTIONAL { ?log chorus:logSourceRetention ?retention } OPTIONAL { ?log chorus:logSourceStatus ?status } } }`;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) return res.status(404).json(athenaEnvelope('subdomain-logs', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    const result = await athenaSparqlQuery(query);
    const logs = result.results.bindings.map((b: any) => ({ uri: b.log.value, label: b.label?.value || b.log.value.split('#').pop(), location: b.location?.value || null, retention: b.retention?.value || null, status: b.status?.value || null }));
    res.json(athenaEnvelope('subdomain-logs', { subdomain: req.params.id, logs }, Date.now() - start, { count: logs.length }));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-logs', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/subdomains/:id/logs — add log source to subdomain (#1926)
app.post('/api/athena/subdomains/:id/logs', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, location, retention, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-log-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const logId = `${req.params.id}-log-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const logUri = `https://jeffbridwell.com/chorus#${logId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${logUri}> a chorus:LogSource ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasLogSource <${logUri}> . ${location ? `<${logUri}> chorus:logSourceLocation "${location.replace(/"/g, '\\"')}" .` : ''} ${retention ? `<${logUri}> chorus:logSourceRetention "${retention}" .` : ''} ${status ? `<${logUri}> chorus:logSourceStatus "${status.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-log-create', { subdomain: req.params.id, uri: logUri, label, location: location || null, retention: retention || null, status: status || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-log-create', { error: err.message }, Date.now() - start, { error: true })); }
});

// GET /api/athena/subdomains/:id/gaps — known gaps for this subdomain (#1926)
app.get('/api/athena/subdomains/:id/gaps', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?gap ?label ?type ?description ?severity WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasGap ?gap . OPTIONAL { ?gap rdfs:label ?label } OPTIONAL { ?gap chorus:gapType ?type } OPTIONAL { ?gap chorus:gapDescription ?description } OPTIONAL { ?gap chorus:gapSeverity ?severity } } }`;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) return res.status(404).json(athenaEnvelope('subdomain-gaps', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    const result = await athenaSparqlQuery(query);
    const gaps = result.results.bindings.map((b: any) => ({ uri: b.gap.value, label: b.label?.value || b.gap.value.split('#').pop(), type: b.type?.value || null, description: b.description?.value || null, severity: b.severity?.value || null }));
    res.json(athenaEnvelope('subdomain-gaps', { subdomain: req.params.id, gaps }, Date.now() - start, { count: gaps.length }));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-gaps', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/subdomains/:id/gaps — add gap to subdomain (#1926)
app.post('/api/athena/subdomains/:id/gaps', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, description, severity } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-gap-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const gapId = `${req.params.id}-gap-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const gapUri = `https://jeffbridwell.com/chorus#${gapId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${gapUri}> a chorus:Gap ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasGap <${gapUri}> . ${type ? `<${gapUri}> chorus:gapType "${type.replace(/"/g, '\\"')}" .` : ''} ${description ? `<${gapUri}> chorus:gapDescription "${description.replace(/"/g, '\\"')}" .` : ''} ${severity ? `<${gapUri}> chorus:gapSeverity "${severity.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-gap-create', { subdomain: req.params.id, uri: gapUri, label, type: type || null, description: description || null, severity: severity || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('subdomain-gap-create', { error: err.message }, Date.now() - start, { error: true })); }
});

// GET /api/athena/subdomains/:id/prior-art — prior art for this subdomain (#1907)
app.get('/api/athena/subdomains/:id/prior-art', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const query = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?item ?label ?path ?description WHERE {
        GRAPH <urn:chorus:instances> {
          <${sdUri}> chorus:hasPriorArt ?item .
          ?item rdfs:label ?label .
          OPTIONAL { ?item chorus:filePath ?path }
          OPTIONAL { ?item rdfs:comment ?description }
        }
      }
      ORDER BY ?label
    `;
    const check = await athenaSparqlQuery(`PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`);
    if (check.results.bindings.length === 0) {
      return res.status(404).json(athenaEnvelope('subdomain-prior-art', { error: `Sub-domain '${req.params.id}' not found` }, Date.now() - start, { error: true }));
    }
    const result = await athenaSparqlQuery(query);
    const items = result.results.bindings.map((b: any) => ({
      uri: b.item.value,
      label: b.label?.value || b.item.value.split('#').pop(),
      path: b.path?.value,
      description: b.description?.value,
    }));
    res.json(athenaEnvelope('subdomain-prior-art', { subdomain: req.params.id, items }, Date.now() - start, { count: items.length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-prior-art', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/prior-art — add prior art to subdomain (#1907)
app.post('/api/athena/subdomains/:id/prior-art', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, path, description } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-prior-art-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const itemId = `${req.params.id}-prior-art-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const itemUri = `https://jeffbridwell.com/chorus#${itemId}`;
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${itemUri}> a chorus:PriorArt ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasPriorArt <${itemUri}> .
          ${path ? `<${itemUri}> chorus:filePath "${path.replace(/"/g, '\\"')}" .` : ''}
          ${description ? `<${itemUri}> rdfs:comment "${description.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-prior-art-create', { subdomain: req.params.id, uri: itemUri, label, path: path || null, description: description || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-prior-art-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/actors — add actor to subdomain (#1899)
app.post('/api/athena/subdomains/:id/actors', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, role, action } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-actor-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const actorId = `${req.params.id}-actor-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const actorUri = `https://jeffbridwell.com/chorus#${actorId}`;
    const roleUri = role ? `chorus:${role}` : '';
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${actorUri}> a chorus:Actor ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasActor <${actorUri}> .
          ${roleUri ? `<${actorUri}> chorus:actorRole ${roleUri} .` : ''}
          ${action ? `<${actorUri}> chorus:actorAction "${action.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-actor-create', { subdomain: req.params.id, uri: actorUri, label, role: role || null, action: action || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-actor-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// DELETE /api/athena/subdomains/:id/:section/:entityId — remove entity from graph (#1929)
// Generic handler for all entity types
const ENTITY_SECTIONS: Record<string, { hasProperty: string; class: string }> = {
  actors: { hasProperty: 'hasActor', class: 'Actor' },
  scenarios: { hasProperty: 'hasScenario', class: 'Scenario' },
  contract: { hasProperty: 'hasContract', class: 'Contract' },
  'prior-art': { hasProperty: 'hasPriorArt', class: 'PriorArt' },
  pages: { hasProperty: 'hasPage', class: 'Page' },
  integrations: { hasProperty: 'hasIntegration', class: 'Integration' },
  persistence: { hasProperty: 'hasPersistence', class: 'PersistenceStore' },
  services: { hasProperty: 'hasService', class: 'Service' },
  pipeline: { hasProperty: 'hasPipeline', class: 'Pipeline' },
  logs: { hasProperty: 'hasLogSource', class: 'LogSource' },
  gaps: { hasProperty: 'hasGap', class: 'Gap' },
};

app.delete('/api/athena/subdomains/:id/:section/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  const { id, section, entityId } = req.params;
  const sectionMeta = ENTITY_SECTIONS[section];
  if (!sectionMeta) return res.status(400).json(athenaEnvelope('entity-delete', { error: `Unknown section: ${section}` }, Date.now() - start, { error: true }));
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${entityId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . <${sdUri}> chorus:${sectionMeta.hasProperty} <${entityUri}> . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await athenaSparqlUpdate(update);
    res.status(204).send();
  } catch (err: any) { res.status(500).json(athenaEnvelope('entity-delete', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/actors/:entityId — update actor (#1929)
app.put('/api/athena/subdomains/:id/actors/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, role, action } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('actor-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    const roleUri = role ? `<https://jeffbridwell.com/chorus#${role}>` : '';
    // Delete all existing triples for this entity
    const del = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await athenaSparqlUpdate(del);
    // Re-insert with updated fields
    const ins = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Actor ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasActor <${entityUri}> . ${roleUri ? `<${entityUri}> chorus:actorRole ${roleUri} .` : ''} ${action ? `<${entityUri}> chorus:actorAction "${action.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(ins);
    res.json(athenaEnvelope('actor-update', { subdomain: req.params.id, uri: entityUri, label, role: role || null, action: action || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('actor-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/scenarios/:entityId — update scenario (#1929)
app.put('/api/athena/subdomains/:id/scenarios/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, given, when, then: thenField, notes } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('scenario-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    const del = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await athenaSparqlUpdate(del);
    const ins = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Scenario ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasScenario <${entityUri}> . ${given ? `<${entityUri}> chorus:scenarioGiven "${given.replace(/"/g, '\\"')}" .` : ''} ${when ? `<${entityUri}> chorus:scenarioWhen "${when.replace(/"/g, '\\"')}" .` : ''} ${thenField ? `<${entityUri}> chorus:scenarioThen "${thenField.replace(/"/g, '\\"')}" .` : ''} ${notes ? `<${entityUri}> chorus:scenarioNotes "${notes.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(ins);
    res.json(athenaEnvelope('scenario-update', { subdomain: req.params.id, uri: entityUri, label, given: given || null, when: when || null, then: thenField || null, notes: notes || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('scenario-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/contract/:entityId — update contract (#1929)
app.put('/api/athena/subdomains/:id/contract/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, path: ep, endpoint, method, description } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('contract-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    const epVal = ep || endpoint || '';
    const del = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await athenaSparqlUpdate(del);
    const ins = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Contract ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasContract <${entityUri}> . ${epVal ? `<${entityUri}> chorus:endpoint "${epVal.replace(/"/g, '\\"')}" .` : ''} ${method ? `<${entityUri}> chorus:httpMethod "${method}" .` : ''} ${description ? `<${entityUri}> chorus:contractDescription "${description.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(ins);
    res.json(athenaEnvelope('contract-update', { subdomain: req.params.id, uri: entityUri, label, path: epVal || null, method: method || null, description: description || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('contract-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/pages/:entityId (#1929)
app.put('/api/athena/subdomains/:id/pages/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, route, description, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('page-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Page ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPage <${entityUri}> . ${route ? `<${entityUri}> chorus:pageRoute "${route.replace(/"/g, '\\"')}" .` : ''} ${description ? `<${entityUri}> chorus:pageDescription "${description.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${entityUri}> chorus:pageStatus "${status.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('page-update', { subdomain: req.params.id, uri: entityUri, label, route: route || null, description: description || null, status: status || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('page-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/integrations/:entityId (#1929)
app.put('/api/athena/subdomains/:id/integrations/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, source, path: dataPath, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('integration-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Integration ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasIntegration <${entityUri}> . ${source ? `<${entityUri}> chorus:integrationSource "${source.replace(/"/g, '\\"')}" .` : ''} ${dataPath ? `<${entityUri}> chorus:integrationPath "${dataPath.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${entityUri}> chorus:integrationStatus "${status.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('integration-update', { subdomain: req.params.id, uri: entityUri, label, source: source || null, path: dataPath || null, status: status || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('integration-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/persistence/:entityId (#1929)
app.put('/api/athena/subdomains/:id/persistence/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, namespace, records, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('persistence-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:PersistenceStore ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPersistence <${entityUri}> . ${type ? `<${entityUri}> chorus:storeType "${type.replace(/"/g, '\\"')}" .` : ''} ${namespace ? `<${entityUri}> chorus:storeNamespace "${namespace.replace(/"/g, '\\"')}" .` : ''} ${records != null ? `<${entityUri}> chorus:storeRecordCount "${records}" .` : ''} ${status ? `<${entityUri}> chorus:storeStatus "${status.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('persistence-update', { subdomain: req.params.id, uri: entityUri, label, type: type || null, namespace: namespace || null, records: records != null ? parseInt(records) : null, status: status || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('persistence-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/services/:entityId (#1929)
app.put('/api/athena/subdomains/:id/services/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, host, status, health_endpoint } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('service-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Service ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasService <${entityUri}> . ${type ? `<${entityUri}> chorus:serviceType "${type.replace(/"/g, '\\"')}" .` : ''} ${host ? `<${entityUri}> chorus:serviceHost "${host.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${entityUri}> chorus:serviceStatus "${status.replace(/"/g, '\\"')}" .` : ''} ${health_endpoint ? `<${entityUri}> chorus:healthEndpoint "${health_endpoint.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('service-update', { subdomain: req.params.id, uri: entityUri, label, type: type || null, host: host || null, status: status || null, health_endpoint: health_endpoint || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('service-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/pipeline/:entityId (#1929)
app.put('/api/athena/subdomains/:id/pipeline/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, source, harvester, icd, status, last_run } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('pipeline-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Pipeline ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPipeline <${entityUri}> . ${source ? `<${entityUri}> chorus:pipelineSource "${source.replace(/"/g, '\\"')}" .` : ''} ${harvester ? `<${entityUri}> chorus:pipelineHarvester "${harvester.replace(/"/g, '\\"')}" .` : ''} ${icd ? `<${entityUri}> chorus:pipelineICD "${icd.replace(/"/g, '\\"')}" .` : ''} ${status ? `<${entityUri}> chorus:pipelineStatus "${status.replace(/"/g, '\\"')}" .` : ''} ${last_run ? `<${entityUri}> chorus:pipelineLastRun "${last_run}" .` : ''} } }`);
    res.json(athenaEnvelope('pipeline-update', { subdomain: req.params.id, uri: entityUri, label, source: source || null, harvester: harvester || null, icd: icd || null, status: status || null, last_run: last_run || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('pipeline-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/logs/:entityId (#1929)
app.put('/api/athena/subdomains/:id/logs/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, location, retention, status } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('log-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:LogSource ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasLogSource <${entityUri}> . ${location ? `<${entityUri}> chorus:logSourceLocation "${location.replace(/"/g, '\\"')}" .` : ''} ${retention ? `<${entityUri}> chorus:logSourceRetention "${retention}" .` : ''} ${status ? `<${entityUri}> chorus:logSourceStatus "${status.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('log-update', { subdomain: req.params.id, uri: entityUri, label, location: location || null, retention: retention || null, status: status || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('log-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/gaps/:entityId (#1929)
app.put('/api/athena/subdomains/:id/gaps/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, type, description, severity } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('gap-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:Gap ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasGap <${entityUri}> . ${type ? `<${entityUri}> chorus:gapType "${type.replace(/"/g, '\\"')}" .` : ''} ${description ? `<${entityUri}> chorus:gapDescription "${description.replace(/"/g, '\\"')}" .` : ''} ${severity ? `<${entityUri}> chorus:gapSeverity "${severity.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('gap-update', { subdomain: req.params.id, uri: entityUri, label, type: type || null, description: description || null, severity: severity || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('gap-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// PUT /api/athena/subdomains/:id/prior-art/:entityId (#1929)
app.put('/api/athena/subdomains/:id/prior-art/:entityId', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, path: filePath, description } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('prior-art-update', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const entityUri = `https://jeffbridwell.com/chorus#${req.params.entityId}`;
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`);
    await athenaSparqlUpdate(`PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a chorus:PriorArt ; rdfs:label "${label.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasPriorArt <${entityUri}> . ${filePath ? `<${entityUri}> chorus:filePath "${filePath.replace(/"/g, '\\"')}" .` : ''} ${description ? `<${entityUri}> rdfs:comment "${description.replace(/"/g, '\\"')}" .` : ''} } }`);
    res.json(athenaEnvelope('prior-art-update', { subdomain: req.params.id, uri: entityUri, label, path: filePath || null, description: description || null }, Date.now() - start));
  } catch (err: any) { res.status(500).json(athenaEnvelope('prior-art-update', { error: err.message }, Date.now() - start, { error: true })); }
});

// POST /api/athena/subdomains/:id/scenarios — add BDD scenario to subdomain (#1899)
app.post('/api/athena/subdomains/:id/scenarios', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, given, when, then: thenField, notes } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-scenario-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const scenarioId = `${req.params.id}-scenario-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const scenarioUri = `https://jeffbridwell.com/chorus#${scenarioId}`;
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${scenarioUri}> a chorus:Scenario ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasScenario <${scenarioUri}> .
          ${given ? `<${scenarioUri}> chorus:scenarioGiven "${given.replace(/"/g, '\\"')}" .` : ''}
          ${when ? `<${scenarioUri}> chorus:scenarioWhen "${when.replace(/"/g, '\\"')}" .` : ''}
          ${thenField ? `<${scenarioUri}> chorus:scenarioThen "${thenField.replace(/"/g, '\\"')}" .` : ''}
          ${notes ? `<${scenarioUri}> chorus:scenarioNotes "${notes.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-scenario-create', { subdomain: req.params.id, uri: scenarioUri, label, given: given || null, when: when || null, then: thenField || null, notes: notes || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-scenario-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/subdomains/:id/contract — add API contract endpoint to subdomain (#1899)
app.post('/api/athena/subdomains/:id/contract', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const { label, path: endpointPath, endpoint, method, description } = req.body || {};
    if (!label) return res.status(400).json(athenaEnvelope('subdomain-contract-create', { error: 'Missing required field: label' }, Date.now() - start, { error: true }));
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;
    const contractId = `${req.params.id}-contract-${label.toLowerCase().replace(/\s+/g, '-')}`;
    const contractUri = `https://jeffbridwell.com/chorus#${contractId}`;
    const ep = endpointPath || endpoint || '';
    const update = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      INSERT DATA {
        GRAPH <urn:chorus:instances> {
          <${contractUri}> a chorus:Contract ;
            rdfs:label "${label.replace(/"/g, '\\"')}" .
          <${sdUri}> chorus:hasContract <${contractUri}> .
          ${ep ? `<${contractUri}> chorus:endpoint "${ep.replace(/"/g, '\\"')}" .` : ''}
          ${method ? `<${contractUri}> chorus:httpMethod "${method}" .` : ''}
          ${description ? `<${contractUri}> chorus:contractDescription "${description.replace(/"/g, '\\"')}" .` : ''}
        }
      }
    `;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-contract-create', { subdomain: req.params.id, uri: contractUri, label, endpoint: endpoint || null, method: method || null }, Date.now() - start));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-contract-create', { error: err.message }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/completeness — lifecycle-gated completeness score (#1899, #1979)
// #1979: Split into 2 parallel queries — metadata (ontology) + instance counts (instances).
// The original monolithic query had 11 OPTIONAL cross-graph joins that caused
// Fuseki timeout on populated domains due to combinatorial explosion.
app.get('/api/athena/subdomains/:id/completeness', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${req.params.id}`;

    // Query 1: Metadata from ontology graph (no cross-graph joins)
    const metaQuery = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?label ?comment ?ownerLabel ?stepLabel
        (COUNT(DISTINCT ?consumed) AS ?consumesCount)
        (COUNT(DISTINCT ?consumer) AS ?consumedByCount)
      WHERE {
        GRAPH <urn:chorus:ontology> {
          <${sdUri}> a chorus:SubDomain .
          OPTIONAL { <${sdUri}> rdfs:label ?label }
          OPTIONAL { <${sdUri}> rdfs:comment ?comment }
          OPTIONAL { <${sdUri}> chorus:ownedBy ?owner . ?owner rdfs:label ?ownerLabel }
          OPTIONAL { <${sdUri}> chorus:primaryStep ?step . ?step rdfs:label ?stepLabel }
          OPTIONAL { <${sdUri}> chorus:consumes ?consumed }
          OPTIONAL { ?consumer chorus:consumes <${sdUri}> }
        }
      }
      GROUP BY ?label ?comment ?ownerLabel ?stepLabel
    `;

    // Query 2: Instance counts — single graph, no cross-graph joins (#1979)
    const countsQuery = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      SELECT
        (COUNT(DISTINCT ?actor) AS ?actorCount)
        (COUNT(DISTINCT ?scenario) AS ?scenarioCount)
        (COUNT(DISTINCT ?contract) AS ?contractCount)
        (COUNT(DISTINCT ?priorArt) AS ?priorArtCount)
        (COUNT(DISTINCT ?page) AS ?pageCount)
        (COUNT(DISTINCT ?integration) AS ?integrationCount)
        (COUNT(DISTINCT ?service) AS ?serviceCount)
        (COUNT(DISTINCT ?persistence) AS ?persistenceCount)
        (COUNT(DISTINCT ?pipeline) AS ?pipelineCount)
        (COUNT(DISTINCT ?logSource) AS ?logSourceCount)
        (COUNT(DISTINCT ?gap) AS ?gapCount)
      WHERE {
        GRAPH <urn:chorus:instances> {
          OPTIONAL { <${sdUri}> chorus:hasActor ?actor }
          OPTIONAL { <${sdUri}> chorus:hasScenario ?scenario }
          OPTIONAL { <${sdUri}> chorus:hasContract ?contract }
          OPTIONAL { <${sdUri}> chorus:hasPriorArt ?priorArt }
          OPTIONAL { <${sdUri}> chorus:hasPage ?page }
          OPTIONAL { <${sdUri}> chorus:hasIntegration ?integration }
          OPTIONAL { <${sdUri}> chorus:hasService ?service }
          OPTIONAL { <${sdUri}> chorus:hasPersistence ?persistence }
          OPTIONAL { <${sdUri}> chorus:hasPipeline ?pipeline }
          OPTIONAL { <${sdUri}> chorus:hasLogSource ?logSource }
          OPTIONAL { <${sdUri}> chorus:hasGap ?gap }
        }
      }
    `;

    // Run both queries in parallel — no cross-graph join in either
    const [metaResult, countsResult] = await Promise.all([
      athenaSparqlQuery(metaQuery),
      athenaSparqlQuery(countsQuery),
    ]);

    const b = metaResult.results.bindings[0];
    if (!b) {
      return res.status(404).json(athenaEnvelope('subdomain-completeness', {
        error: `Sub-domain '${req.params.id}' not found`,
      }, Date.now() - start, { error: true }));
    }

    const c = countsResult.results.bindings[0] || {};
    const sections: Record<string, boolean> = {
      label: !!b.label,
      comment: !!b.comment,
      owner: !!b.ownerLabel,
      step: !!b.stepLabel,
      actors: parseInt(c.actorCount?.value || '0') > 0,
      scenarios: parseInt(c.scenarioCount?.value || '0') > 0,
      contract: parseInt(c.contractCount?.value || '0') > 0,
      prior_art: parseInt(c.priorArtCount?.value || '0') > 0,
      pages: parseInt(c.pageCount?.value || '0') > 0,
      integrations: parseInt(c.integrationCount?.value || '0') > 0,
      services: parseInt(c.serviceCount?.value || '0') > 0,
      persistence: parseInt(c.persistenceCount?.value || '0') > 0,
      pipeline: parseInt(c.pipelineCount?.value || '0') > 0,
      logs: parseInt(c.logSourceCount?.value || '0') > 0,
      gaps: parseInt(c.gapCount?.value || '0') > 0,
      edges: (parseInt(b.consumesCount?.value || '0') + parseInt(b.consumedByCount?.value || '0')) > 0,
    };

    const lifecycle: Record<string, { required: string[]; met: string[]; missing: string[]; pass: boolean }> = {
      create: { required: ['label', 'owner', 'step', 'comment'], met: [], missing: [], pass: false },
      wip: { required: ['actors', 'edges'], met: [], missing: [], pass: false },
      done: { required: ['scenarios', 'contract'], met: [], missing: [], pass: false },
    };
    for (const [stage, gate] of Object.entries(lifecycle)) {
      gate.met = gate.required.filter(r => sections[r]);
      gate.missing = gate.required.filter(r => !sections[r]);
      gate.pass = gate.missing.length === 0;
    }

    const present = Object.entries(sections).filter(([, v]) => v).map(([k]) => k);
    const missing = Object.entries(sections).filter(([, v]) => !v).map(([k]) => k);
    const percentage = Math.round((present.length / Object.keys(sections).length) * 100);

    res.json(athenaEnvelope('subdomain-completeness', {
      subdomain: req.params.id,
      label: b.label?.value,
      step: b.stepLabel?.value,
      sections,
      present,
      missing,
      percentage,
      lifecycle,
    }, Date.now() - start, { count: present.length, total: Object.keys(sections).length }));
  } catch (err: any) {
    res.status(500).json(athenaEnvelope('subdomain-completeness', { error: err.message }, Date.now() - start, { error: true }));
  }
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
  const start = Date.now();
  const cardId = req.params.id;
  try {
    const cardsScript = path.resolve(__dirname, '../../scripts/cards');
    const { stdout } = await execAsync(
      `bash ${cardsScript} view ${cardId} --json 2>/dev/null`,
      { encoding: 'utf-8', timeout: 10000, env: { ...process.env, PATH: `/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:${process.env.PATH}` } }
    );
    const card = JSON.parse(stdout);
    // Parse AC items from description (#1933)
    const acItems: { text: string; checked: boolean }[] = [];
    if (card.description) {
      const acMatches = card.description.match(/- \[([ x])\] .+/g) || [];
      for (const line of acMatches) {
        const checked = line.startsWith('- [x]');
        const text = line.replace(/^- \[[ x]\] /, '');
        acItems.push({ text, checked });
      }
    }
    card.ac_items = acItems;
    res.json(athenaEnvelope('card-detail', card, Date.now() - start));
  } catch (err: any) {
    res.status(404).json(athenaEnvelope('card-detail', { error: `Card ${cardId} not found` }, Date.now() - start, { error: true }));
  }
});

// 404 handler for unknown /api/athena/* paths — agent-friendly suggestions
app.use('/api/athena', (_req: Request, res: Response) => {
  res.status(404).json(athenaEnvelope('unknown', {
    error: `Unknown Athena endpoint: ${_req.path}`,
    suggestion: 'Use GET /api/athena/health to discover available endpoints.',
    available: ATHENA_QUERIES.map(q => q.path),
  }, 0, { error: true }));
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

const BIND_HOST = process.env.CHORUS_BIND || '0.0.0.0';
app.listen(PORT, BIND_HOST, () => {
  console.log(`[chorus-api] Listening on ${BIND_HOST}:${PORT}`);
  console.log(`[chorus-api] Database: ${DB_PATH}`);
  // Init LanceDB async (non-blocking)
  initLance().catch(err => console.error(`[chorus-api] LanceDB init error: ${err}`));

  // Embed sync moved to standalone worker (chorus-embed-worker.sh) — #1978
  // The in-process timer was blocking the API with 100+ sequential Ollama calls per cycle.
  // POST /api/chorus/embed still works for on-demand batches.
});

export default app;
