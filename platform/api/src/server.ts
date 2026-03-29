import express, { Request, Response, NextFunction } from 'express';
import Database from 'better-sqlite3';
import { execFile } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs';
import * as lancedb from '@lancedb/lancedb';

const app = express();
app.use(express.json());
const PORT = parseInt(process.env.CHORUS_API_PORT || '3340', 10);
const DB_PATH = path.join(os.homedir(), '.chorus', 'index.db');
const LANCE_DIR = path.join(os.homedir(), '.chorus', 'lance');
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';
const EMBED_MODEL = 'nomic-embed-text';
// Prefer repo scripts (always present), fall back to ~/.chorus/scripts
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
const EMBED_BATCH_SIZE = 50;

async function embedDelta(): Promise<{ embedded: number; skipped: number }> {
  if (!lanceDb) {
    await initLance();
    if (!lanceDb) return { embedded: 0, skipped: 0 };
  }

  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');

  try {
    // Find max msg_id already in LanceDB
    let maxEmbeddedId = 0;
    if (lanceTable) {
      try {
        const rows = await lanceTable.query().select(['msg_id']).toArray();
        for (const r of rows) {
          if ((r as any).msg_id > maxEmbeddedId) maxEmbeddedId = (r as any).msg_id;
        }
      } catch { /* empty table */ }
    }

    // Get new messages not yet embedded
    const newMessages = db.prepare(`
      SELECT id, source, channel, role, content, timestamp
      FROM messages
      WHERE id > ? AND LENGTH(content) >= ?
      ORDER BY id ASC
    `).all(maxEmbeddedId, MIN_EMBED_LENGTH) as Array<{
      id: number; source: string; channel: string; role: string; content: string; timestamp: string;
    }>;

    if (newMessages.length === 0) return { embedded: 0, skipped: 0 };

    console.log(`[embed-delta] ${newMessages.length} new messages to embed (after id ${maxEmbeddedId})`);

    const records: Array<{
      msg_id: number; source: string; channel: string; role: string;
      content: string; timestamp: string; vector: number[];
    }> = [];
    let skipped = 0;

    // Embed in batches
    for (let i = 0; i < newMessages.length; i += EMBED_BATCH_SIZE) {
      const batch = newMessages.slice(i, i + EMBED_BATCH_SIZE);
      for (const msg of batch) {
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
        } catch {
          skipped++;
        }
      }
    }

    if (records.length === 0) return { embedded: 0, skipped };

    // Write to LanceDB
    if (lanceTable) {
      await lanceTable.add(records);
    } else if (lanceDb) {
      lanceTable = await lanceDb.createTable('messages', records);
    }

    console.log(`[embed-delta] Embedded ${records.length}, skipped ${skipped}`);
    return { embedded: records.length, skipped };
  } finally {
    db.close();
  }
}

async function embedQuery(text: string): Promise<number[]> {
  const res = await fetch(`${OLLAMA_URL}/api/embeddings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model: EMBED_MODEL, prompt: text }),
    signal: AbortSignal.timeout(15000),
  });
  if (!res.ok) throw new Error(`Ollama embed failed: ${res.status}`);
  const data = await res.json() as { embedding: number[] };
  return data.embedding;
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
const CHORUS_LOG = path.join(os.homedir(), 'CascadeProjects/chorus/platform/scripts/chorus-log.sh');

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
      res.json({ results, total: results.length, mode: 'semantic' });
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
        res.json({ results: merged, total: merged.length, mode: 'unified', sources: { fts: ftsResults.length, semantic: semResults.length, sparql: sparqlResults.length } });
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
        res.json({ results: merged, total: merged.length, mode: 'hybrid' });
        return;
      } catch {
        // Semantic failed — fall through to FTS-only
      }
    }

    emitSearchEvent({ system: 'chorus-api', query: q.slice(0, 200), mode: 'fts', result_count: ftsResults.length, duration_ms: Date.now() - searchStart, ...(role ? { role_filter: role } : {}) });
    res.json({ results: ftsResults, total: ftsResults.length, mode: 'fts' });
  } finally {
    db.close();
  }
});

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

// --- POST /api/chorus/index ---

app.post('/api/chorus/index', (_req: Request, res: Response) => {
  const startTime = Date.now();
  const indexers = [
    { name: 'slack', script: 'chorus-index-slack.sh' },
    { name: 'sessions', script: 'chorus-index-sessions.sh' },
    { name: 'artifacts', script: 'chorus-index-artifacts.sh' },
    { name: 'spine', script: 'chorus-index-spine.sh' },
    { name: 'stories', script: 'chorus-index-stories.sh' },
    { name: 'journal', script: 'chorus-index-journal.sh' },
  ];

  const results: Record<string, string> = {};
  let completed = 0;

  for (const indexer of indexers) {
    const scriptPath = path.join(SCRIPTS_DIR, indexer.script);
    execFile('bash', [scriptPath], { timeout: 30000 }, (err, stdout, stderr) => {
      if (err) {
        results[indexer.name] = `error: ${err.message}`;
      } else {
        results[indexer.name] = stdout.trim();
      }
      completed++;
      if (completed === indexers.length) {
        // Run ref extraction after indexers
        const refsScript = path.join(SCRIPTS_DIR, 'chorus-extract-refs.sh');
        execFile('bash', [refsScript], { timeout: 15000 }, (refErr, refOut) => {
          const elapsed = Date.now() - startTime;
          res.json({
            indexed: results,
            refs: refErr ? `error: ${refErr.message}` : refOut.trim(),
            elapsed_ms: elapsed
          });
          // Embed new messages async — don't block the response
          embedDelta().catch(err =>
            console.error(`[embed-delta] post-index embed failed: ${err.message}`)
          );
        });
      }
    });
  }
});

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

// --- POST /api/chorus/alert (Grafana webhook receiver) ---

app.post('/api/chorus/alert', (req: Request, res: Response) => {
  const CHORUS_LOG = '/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log';
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
    // Hour of day (Boston = UTC-5)
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

      // Hour of day (UTC-5 Boston)
      const utcHour = d.getUTCHours();
      const bostonHour = (utcHour - 5 + 24) % 24;
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

    // --- Daily rhythm: hour-of-day buckets (Boston = UTC-5) ---
    const hourBuckets: { keys: number[]; prompts: number[]; count: number[] } = {
      keys: Array(24).fill(0),
      prompts: Array(24).fill(0),
      count: Array(24).fill(0),
    };
    day.forEach(r => {
      const hr = new Date((r.timestamp - 5 * 3600) * 1000).getUTCHours();
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
  execFile('/usr/sbin/diskutil', ['info', '/'], { timeout: 10000 }, (err, stdout) => {
    if (err) {
      res.status(500).json({ error: 'diskutil info failed', detail: err.message });
      return;
    }

    const extract = (label: string): string | null => {
      const match = stdout.match(new RegExp(`${label}:\\s*(.+)`));
      return match ? match[1].trim() : null;
    };

    const totalSize = extract('Container Total Space');
    const freeSize = extract('Container Free Space');

    // Parse bytes from strings like "1.8 TB (2000000000000 Bytes)"
    const parseBytes = (s: string | null): number | null => {
      if (!s) return null;
      const m = s.match(/\((\d+)\s*Bytes\)/);
      return m ? parseInt(m[1], 10) : null;
    };

    const totalBytes = parseBytes(totalSize);
    const freeBytes = parseBytes(freeSize);
    const usedBytes = totalBytes && freeBytes ? totalBytes - freeBytes : null;
    const usedPct = totalBytes && usedBytes ? Math.round((usedBytes / totalBytes) * 100) : null;

    res.json({
      machine: 'Library',
      total: totalSize,
      free: freeSize,
      total_bytes: totalBytes,
      free_bytes: freeBytes,
      used_bytes: usedBytes,
      used_pct: usedPct,
      warning: usedPct !== null && usedPct >= 90,
      critical: usedPct !== null && usedPct >= 95,
    });
  });
});

// --- GET /api/chorus/harvest — Harvest pipeline status (#1485) ---

const HARVEST_EXPORTER = path.join(os.homedir(), 'CascadeProjects/chorus/platform/scripts/harvest-exporter.sh');

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

const COST_SCRIPT = path.join(os.homedir(), 'CascadeProjects/chorus/platform/scripts/cost-report.sh');

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

// --- Health check ---

app.get('/health', (_req: Request, res: Response) => {
  try {
    const db = getDb();
    const count = (db.prepare(`SELECT COUNT(*) as c FROM messages`).get() as any).c;
    db.close();
    res.json({ status: 'ok', messages: count });
  } catch {
    res.status(503).json({ status: 'unhealthy' });
  }
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

const BIND_HOST = process.env.CHORUS_BIND || '0.0.0.0';
app.listen(PORT, BIND_HOST, () => {
  console.log(`[chorus-api] Listening on ${BIND_HOST}:${PORT}`);
  console.log(`[chorus-api] Database: ${DB_PATH}`);
  // Init LanceDB async (non-blocking)
  initLance().catch(err => console.error(`[chorus-api] LanceDB init error: ${err}`));
});

export default app;
