/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * fs paths in this server file are all server-controlled:
 *   - Constants: DB_PATH, CHORUS_LOG, CHORUS_ROOT, REPO_ROOT, /tmp/pulse-latest.json
 *   - Joins from os.homedir(), __dirname, path.resolve under app root
 *   - Filenames discovered via fs.readdirSync of trusted view/doc directories
 * Object/array indexing is on internally-derived keys (aliasToId map keyed
 * by regex-extracted view filenames). User input from req.params is forwarded
 * to handlers, which validate before reaching their own fs/index sinks.
 */
import express, { Request, Response, NextFunction } from 'express';
import Database from 'better-sqlite3';
import { execFile, exec, spawn, fork as forkChild } from 'child_process';
import { promisify } from 'util';
import path from 'path';
import os from 'os';
import fs from 'fs';
// #3089: hoisted from the later import block so makeRequestOpMiddleware is
// initialized before the `app.use(makeRequestOpMiddleware())` call below
// (CJS compile evaluates imports in source order → TDZ if used before).
import { startEventloopAlert, setCurrentOp, makeRequestOpMiddleware, firstAppFrame } from './eventloop-alert';

const execAsync = promisify(exec);
// #3197 — single root source. Replaces a wrong inline default
// ('/CascadeProjects', missing the /chorus segment) that had grown three pairs
// of compensating dual-path lookups below; with the root always correct, the
// `${ROOT}/chorus/...` and `${ROOT}/shared-observability` branches are dead and
// have been removed.
import { CHORUS_ROOT } from './lib/chorus-paths';

/** Extract a string message from an unknown error. #2463 wave 1: replaces `catch (err: any)` + `err.message`. */
function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** SPARQL binding row — each variable maps to a { value, type } cell. */
type SparqlBinding = Record<string, { value: string; type?: string; datatype?: string }>;

const app = express();
app.use(express.json());

// #3089: name every request so eventloop.blocked alerts attribute to a route
// (`op=GET /api/chorus/search`) instead of `op=unknown`. Must come BEFORE any
// route handlers so it fires on entry; clearing on `finish` + `close` so the op
// doesn't leak past the response.
app.use(makeRequestOpMiddleware());

// #3618 — the security envelope: gates model-declared mutation surfaces.
// Mounted here (before route handlers) but DEPLOY-SAFE: enabled only when
// CHORUS_SECURITY_ENVELOPE_ENABLE=1 (the flip step, after a surface's consumers
// carry credentials). Off = pure pass-through. The surface table is loaded
// async from the model below (after the SPARQL client exists) and swapped into
// this mutable ref without re-mounting; empty until loaded = gates nothing.
import { securityEnvelope, type SecuredSurface } from './security-envelope';
import { projectSecuredSurfaces } from './security-surfaces-emit';
let SECURED_SURFACES: SecuredSurface[] = [];
app.use(securityEnvelope({
  getSurfaces: () => SECURED_SURFACES,
  secret: process.env.CHORUS_SERVICE_TOKEN_SECRET ?? '',
  nowSecs: () => Math.floor(Date.now() / 1000),
  enabled: process.env.CHORUS_SECURITY_ENVELOPE_ENABLE === '1',
  emit: (event: string, fields: Record<string, string>) => {
    recordEnvelopeEvent(event, fields); // #3628 — refusals become a scrapeable counter
    const args = [event, 'silas'];
    for (const [k, v] of Object.entries(fields)) args.push(`${k}=${v}`);
    execFile(CHORUS_LOG, args, () => { /* fire-and-forget */ });
  },
}));

// #2998 — MCP transport REMOVED from chorus-api. /mcp now served by the
// chorus-mcp daemon on :3341 (separate LaunchAgent com.chorus.mcp). Decouples
// MCP from chorus-api's deploy lifecycle so chorus-api redeploys no longer
// kill role-session MCP transports. See #2997 for the daemon stand-up.
// platform/api/src/mcp/ stays in the tree for now — chorus-mcp consumes a
// copy in platform/mcp-server/. Removing this dir is a separate retirement
// card after the source-of-truth swap is verified stable.

import { getHooksSummary } from './hooks-summary';
import { getCostSummary } from './cost-summary';
import { startMetrics, getMetrics, recordEnvelopeEvent } from './metrics';
import { formatAccessLine } from './access-log';
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

// #3656 — Quality Service reparented borg→loom (ADR-026: quality layers are team
// practice, loom's charter). Old borg path redirects so bookmarks/links survive.
app.get(['/borg/quality', '/borg/quality/', '/borg/quality/index.html'], (_req: Request, res: Response) =>
  res.redirect(301, '/loom/quality/'));

// #2458 — serve migrated chorus artifacts at their new canonical locations
// (cross-repo moves from jeff-bridwell-personal-site completed in #2458)
const chorusRepoRoot = path.resolve(__dirname, '..', '..', '..');
app.use('/designing/docs', express.static(path.join(chorusRepoRoot, 'designing', 'docs'), { extensions: ['html'] }));
// #2704 — doc-catalog announces hrefs under all designing/* subdirs but only
// /designing/docs was mounted. Decisions/claudemd/domain-context/schemas
// 404'd, polluting the doc-coherence broken-hrefs ratchet with non-bugs.
app.use('/designing/decisions', express.static(path.join(chorusRepoRoot, 'designing', 'decisions'), { extensions: ['html', 'md'] }));
app.use('/designing/claudemd', express.static(path.join(chorusRepoRoot, 'designing', 'claudemd'), { extensions: ['html', 'md'] }));
app.use('/designing/domain-context', express.static(path.join(chorusRepoRoot, 'designing', 'domain-context'), { extensions: ['html', 'md'] }));
app.use('/designing/schemas', express.static(path.join(chorusRepoRoot, 'designing', 'schemas'), { extensions: ['html', 'md', 'json'] }));
app.use('/roles/silas/adr', express.static(path.join(chorusRepoRoot, 'roles', 'silas', 'adr'), { extensions: ['html', 'md'] }));
app.use('/roles/silas/artifacts', express.static(path.join(chorusRepoRoot, 'roles', 'silas', 'artifacts'), { extensions: ['html'] }));
app.use('/roles/kade/artifacts', express.static(path.join(chorusRepoRoot, 'roles', 'kade', 'artifacts'), { extensions: ['html'] }));
// #3361 — value-stream tree mount: chorus UI pages moved home from gathering
// land under building/ (e.g. building/products/werk/werk-process,
// building/products/convergence/nifi-chorus-integration-design). Served here so
// the page lives in its ADR-041 home, not in gathering.
app.use('/building', express.static(path.join(chorusRepoRoot, 'building'), { extensions: ['html'] }));

// #3361 — chorus pages moved home from gathering. Served as PRE-RENDERED STATIC
// HTML (rendered from the relocated EJS views by render-chorus-pages.cjs, output
// to public/chorus-pages/). No runtime template engine: a new runtime npm dep
// (ejs) does not survive the chorus-api deploy's node_modules lifecycle, so the
// pages serve as committed static files — nothing for the pipeline to drop, and
// the html lands in canon via git merge + the existing public/ static serving.
// Live/dynamic data wiring is the prioritized follow-on (#3361); these are shells
// or empty-data snapshots until then (Jeff: move home now, fix data later).
const chorusPageDir = path.join(__dirname, '..', 'public', 'chorus-pages');
const sendChorusPage = (file: string) =>
  (_req: Request, res: Response) => res.sendFile(path.join(chorusPageDir, file));
app.get('/chorus', sendChorusPage('chorus.html'));
app.get('/chorus/system', (_req: Request, res: Response) => res.redirect(301, '/chorus'));
app.get('/chorus-model-data', sendChorusPage('chorus-model-data.html'));
app.get('/borg-assessment', sendChorusPage('borg-assessment.html'));
app.get('/harvesting/icd', sendChorusPage('icd.html'));
app.get('/harvesting/convergence', sendChorusPage('icd.html'));
app.get('/harvesting/mapper', (_req: Request, res: Response) => res.redirect(301, '/harvesting/icd'));
app.get('/werk', sendChorusPage('werk.html'));
app.get('/harvest-manifests', sendChorusPage('harvest-manifests.html'));
app.get('/loom', sendChorusPage('loom.html'));
app.get('/loom/:role', sendChorusPage('loom.html'));
app.get('/flow', sendChorusPage('flow.html'));
app.get('/model-data', sendChorusPage('model-data.html'));
app.get('/ontology-views/:domain', sendChorusPage('model-data.html'));
// #3408 — re-live the /werk cockpit. Its client JS fetches these; they 404'd
// after the #3361 page move. Schema = the spine-events vocab, already in the
// chorus tree at designing/schemas (served straight from disk, no new data dep).
app.get('/api/werk/schema', (_req: Request, res: Response) => {
  const p = path.join(chorusRepoRoot, 'designing', 'schemas', 'spine-events.json');
  fs.readFile(p, 'utf-8', (err, raw) => {
    if (err) { res.status(404).json({ error: 'spine-events.json not found' }); return; }
    res.type('json').send(raw);
  });
});
// #3429 — safe stringify for spine-log fields (parsed as Record<string,unknown>):
// primitives render directly, objects as JSON (not "[object Object]"), null/undefined as "".
function str(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v); // objects/arrays as JSON, not "[object Object]"
}
// #3429 — one spine line → an activity entry (or null to skip). Extracted so the
// /api/werk/activity handler stays under the complexity threshold.
function activityEntry(line: string, cutoffMs: number, roleF: string | null, eventF: string[] | null): Record<string, string> | null {
  if (!line.trim()) return null;
  let p: Record<string, unknown>;
  try { p = JSON.parse(line); } catch { return null; }
  const ts = Date.parse(str(p.timestamp));
  if (!Number.isNaN(ts) && ts < cutoffMs) return null;
  if (roleF && p.role !== roleF) return null;
  if (eventF && !eventF.includes(str(p.event))) return null;
  return {
    timestamp: str(p.timestamp),
    event: str(p.event),
    role: str(p.role),
    card_id: str(p.card_id ?? p.cardId),
    workflow_id: str(p.workflow_id ?? p.workflowId),
    gate: str(p.gate),
    stage: str(p.stage),
    board: str(p.board),
    title: str(p.title),
  };
}
// #3429 — loom-metrics accumulator + one-line tally, extracted so the
// /api/loom-metrics handler stays under the complexity threshold.
interface LoomAcc { done: Set<string>; pulled: Set<string>; weekly: Record<string, number>; deploys: number; demoTotal: number; }
function loomTally(acc: LoomAcc, line: string): void {
  if (!line.trim()) return;
  let p: Record<string, unknown>;
  try { p = JSON.parse(line); } catch { return; }
  const ev = str(p.event);
  const cid = str(p.card_id ?? p.cardId);
  if (ev === 'card.accepted' || ev === 'card_done' || ev === 'card.done') {
    if (cid) acc.done.add(cid);
    const d = new Date(str(p.timestamp));
    if (!Number.isNaN(d.getTime())) {
      const wk = `${String(d.getMonth() + 1).padStart(2, '0')}/${String(d.getDate()).padStart(2, '0')}`;
      acc.weekly[wk] = (acc.weekly[wk] ?? 0) + 1;
    }
  }
  if (ev === 'card.pulled' && cid) acc.pulled.add(cid);
  if (ev.includes('deploy') || ev === 'binary.deployed') acc.deploys++;
  // #3410 — demo.verdict synthesis retired; demoTotal counts REAL demos presented.
  if (ev === 'demo.presented') acc.demoTotal++;
}
// #3408 — /werk activity feed: the live spine event stream (the cockpit's heart).
// Reads the chorus.log tail (same bounded source as /context/spine — #3406) and
// returns {entries:[...]} filtered by role/event/hours, the shape werk.html's JS
// consumes. No new data dependency; the spine IS chorus-api's own event log.
app.get('/api/werk/activity', (req: Request, res: Response) => {
  const hours = Math.max(1, parseInt(str(req.query.hours) || '168', 10) || 168);
  const roleF = typeof req.query.role === 'string' && req.query.role ? req.query.role : null;
  const eventF = typeof req.query.event === 'string' && req.query.event ? String(req.query.event).split('|') : null;
  const cutoffMs = Date.now() - hours * 3600 * 1000;
  const raw = readFileTail(`${process.env.HOME}/.chorus/chorus.log`, SPINE_TAIL_BYTES);
  if (raw == null) { res.json({ entries: [], error: 'spine log unavailable' }); return; }
  const entries: Array<Record<string, string>> = [];
  for (const line of raw.split('\n')) {
    const e = activityEntry(line, cutoffMs, roleF, eventF);
    if (e) entries.push(e);
  }
  // #3408 — bound the RESPONSE too (Silas's #3406-forward flag): a polling cockpit
  // doesn't need the whole 4MB-tail's worth; return the most-recent N (default 500)
  // so the payload stays small as the spine grows. ?limit overrides (cap 2000).
  const limit = Math.min(2000, Math.max(1, parseInt(str(req.query.limit) || '500', 10) || 500));
  res.json({ entries: entries.slice(-limit) });
});
// #3408 — /werk fitness panel metrics, computed live from the spine tail (no new
// data store): weekly throughput (cards accepted per ISO-week), reject rate
// (no-go demos / total demos), deploys, and done/pulled completion. Shapes match
// werk.html renderFitnessPanel (board / weekly_throughput / reject_stats / operations).
app.get('/api/loom-metrics', (_req: Request, res: Response) => {
  const raw = readFileTail(`${process.env.HOME}/.chorus/chorus.log`, SPINE_TAIL_BYTES);
  const acc: LoomAcc = { done: new Set<string>(), pulled: new Set<string>(), weekly: {}, deploys: 0, demoTotal: 0 };
  if (raw != null) {
    for (const line of raw.split('\n')) loomTally(acc, line);
  }
  const demoNoGo = 0; // #3410 — no-go isn't carried on demo.presented; stays 0 until a real no-go signal exists
  const total = new Set([...acc.done, ...acc.pulled]).size;
  res.json({
    board: { done: acc.done.size, total },
    weekly_throughput: acc.weekly,
    reject_stats: { rate: acc.demoTotal > 0 ? Math.round((demoNoGo / acc.demoTotal) * 1000) / 10 : 0, deploys: acc.deploys },
    operations: { deploys: acc.deploys },
  });
});
// #2994 — additional role mounts. doc-catalog registered these paths but
// chorus-api had no static mounts; files exist on disk, hrefs 404'd.
app.use('/roles/silas/docs', express.static(path.join(chorusRepoRoot, 'roles', 'silas', 'docs'), { extensions: ['html', 'md'] }));
app.use('/roles/wren/artifacts', express.static(path.join(chorusRepoRoot, 'roles', 'wren', 'artifacts'), { extensions: ['html', 'md'] }));
app.use('/roles/wren/docs', express.static(path.join(chorusRepoRoot, 'roles', 'wren', 'docs'), { extensions: ['html', 'md'] }));
app.use('/roles/wren/decisions', express.static(path.join(chorusRepoRoot, 'roles', 'wren', 'decisions'), { extensions: ['html', 'md'] }));
// #2994 — /skills/<name>/SKILL.md routing. #2969 doc-catalog-add registered
// 40+ skill SKILL.md hrefs as /skills/<name>/SKILL.md, but chorus-api had no
// static mount serving them; they 404'd and inflated broken-hrefs to 68.
// Same pattern as the /designing/{decisions,claudemd,domain-context,schemas}
// fix in #2704: routes are the gap, not the catalog. Canonical skills tree
// is at chorus/skills (matches doc-catalog.ts scan dir at line 105).
app.use('/skills', express.static(path.join(chorusRepoRoot, 'skills'), { extensions: ['md', 'html'] }));
// #2994 — /diagrams/* mount. doc-catalog.ts line 105 registers urlPrefix
// '/diagrams/' for files in docs/diagrams; the mount was missing. Catches
// chorus-c4.html, skills-hooks-mapping.html, etc.
app.use('/diagrams', express.static(path.join(chorusRepoRoot, 'docs', 'diagrams'), { extensions: ['html', 'md'] }));

// #2445 wave 2 — chorus-api serves catalog HTML. Static routes try chorus
// designing/docs first (where 43 misfiled docs were relocated post-#2510),
// then fall back to gathering filesystem for content that legitimately
// lives there (gathering-domain pages, akasha consulting site).
// Express runs middleware in registration order; first hit wins.
const gatheringRepoRoot = process.env.GATHERING_REPO || '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site';
app.use('/system/docs', express.static(path.join(chorusRepoRoot, 'designing', 'docs'), { extensions: ['html', 'md'] }));
app.use('/system/docs', express.static(path.join(gatheringRepoRoot, 'data', 'about'), { extensions: ['html', 'md'] }));
app.use('/chorus-docs', express.static(path.join(chorusRepoRoot, 'designing', 'docs'), { extensions: ['html'] }));
app.use('/chorus-docs', express.static(path.join(gatheringRepoRoot, 'public', 'chorus-docs'), { extensions: ['html'] }));
app.use('/gathering-docs', express.static(path.join(chorusRepoRoot, 'designing', 'docs'), { extensions: ['html'] }));
app.use('/gathering-docs', express.static(path.join(gatheringRepoRoot, 'public', 'gathering-docs'), { extensions: ['html'] }));
app.use('/akasha', express.static(path.join(gatheringRepoRoot, 'public', 'akasha'), { extensions: ['html'] }));

// Borg — Hooks summary endpoint — #2099
// Borg summary delegates — #2173 AC4: uniform run() wrapper replaces
// per-handler try/catch boilerplate. Each adapter is one line.
import { run, asStr } from './handlers/util';

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

// #3029 — pain board endpoints. Same queryPainRollup the MCP tool + regression
// test use (one source, can't drift). Validated contract: {job} selector +
// event-field anchor. The HTML pages under public/borg/ consume these.
import { queryPainRollup, type RollupWindow, logsForCard, logsForTrace } from './handlers/logs-query';
const painLokiDeps = {
  fetchImpl: fetch,
  lokiUrl: process.env.LOKI_URL ?? 'http://localhost:3102',
  now: () => Date.now(),
  // #3149-fix — read-time domain derivation: resolve a card_id to its domain from
  // the LIVE board-cache (getBoardCards refreshes on an interval). No stale index,
  // no emit-time stamping — the rollup joins card_id -> current domain at query time.
  domainOf: (id: string): string | undefined => getBoardCards().find((c) => c.id === id)?.domain || undefined,
};
app.get('/api/chorus/pain/rollup', async (req, res) => {
  // Pass the raw window through; queryPainRollup is the single validator (don't
  // silently coerce a bad window to 7d — that returns mislabeled data). Invalid
  // window → typed time-range-invalid → 400; Loki down → 502.
  const window = (typeof req.query.window === 'string' ? req.query.window : '7d') as RollupWindow;
  const result = await queryPainRollup({ window }, painLokiDeps);
  const status = result.ok ? 200 : result.reason === 'time-range-invalid' ? 400 : 502;
  res.status(status).json(result);
});
app.get('/api/chorus/pain/card/:id', async (req, res) => {
  const id = Number(req.params.id);
  if (!Number.isFinite(id)) { res.status(400).json({ ok: false, reason: 'bad-card-id' }); return; }
  const result = await logsForCard({ card_id: id, time_window: '1d' }, painLokiDeps);
  res.status(result.ok ? 200 : 502).json(result);
});

// #3122 — trace-reader data source. Same single-source query the MCP tools use
// (logsForCard / logsForTrace); the /borg/trace.html page renders this verbatim
// so a human view and an agent view can never diverge. Purpose-named routes
// (vs /pain/card which is failure-triage-flavored) — same function underneath.
app.get('/api/chorus/logs/card/:id', async (req, res) => {
  const id = Number(req.params.id);
  if (!Number.isFinite(id)) { res.status(400).json({ ok: false, reason: 'bad-card-id' }); return; }
  const result = await logsForCard({ card_id: id, time_window: '1d' }, painLokiDeps);
  res.status(result.ok ? 200 : 502).json(result);
});
app.get('/api/chorus/logs/trace/:traceId', async (req, res) => {
  const traceId = req.params.traceId;
  if (!traceId) { res.status(400).json({ ok: false, reason: 'bad-trace-id' }); return; }
  const result = await logsForTrace({ trace_id: traceId, time_window: '1d' }, painLokiDeps);
  res.status(result.ok ? 200 : 502).json(result);
});

// New canonical path under /api/chorus/context/* (#2252 migration).
app.get('/api/chorus/context/quality/summary', async (req, res) => {
  const r = await fetchContextQualitySummary(
    { sparql: _athena, runQuality: () => run(() => getQualityScan()) },
    req.originalUrl,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/quality/domain/:domain', async (req, res) => {
  const domain = String(req.params.domain || '').toLowerCase();
  const r = await run(() => getQualityByDomain(domain));
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/patterns/summary', async (req, res) => {
  const days = parseInt(asStr(req.query.days, '30'), 10) || 30;
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
  const days = parseInt(asStr(req.query.days, '7'), 10) || 7;
  const posture = asStr(req.query.posture, 'all');
  const mood = asStr(req.query.mood, 'all');
  const r = await run(() => getPostureStrip(days, posture, mood));
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/werk/activity', async (req, res) => {
  const hours = parseInt(asStr(req.query.hours, '24'), 10) || 24;
  const role = asStr(req.query.role, '');
  const event = asStr(req.query.event, '');
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

app.get('/api/chorus/sessions', async (_req, res) => {
  const r = await fetchSessionList(sessionDeps);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/sessions/:id', async (req, res) => {
  const r = await fetchSessionById(sessionDeps, req.params.id);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/sessions/:id/log', async (req, res) => {
  const r = await fetchSessionLog(sessionDeps, req.params.id);
  if (r.contentType === 'text/plain') {
    res.status(r.status).type('text/plain').send(r.body);
    return;
  }
  res.status(r.status).json(r.body);
});

// Request logging — every request writes to stdout so the log stays fresh.
// The line carries the request START timestamp (#3058) so an event-loop-block
// alert at time T can be matched to the request whose [start, start+ms]
// window contains T. Format is the pure formatAccessLine() (unit-tested).
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => {
    const ms = Date.now() - start;
    console.log(formatAccessLine(start, req.method, req.path, res.statusCode, ms));
  });
  next();
});
const PORT = parseInt(process.env.CHORUS_API_PORT || '3340', 10);
// #3379 — env-overridable: demo VARIANTS must not open production's stores
// (three api processes sharing index.db+lance was the day's wedge root; the
// env-up isolation follow-on passes per-werk paths through these seams).
const DB_PATH = process.env.CHORUS_DB_PATH || path.join(os.homedir(), '.chorus', 'index.db');
// #3382 — CHORUS_LANCE_DIR is now read by the search worker (off-process), not here.
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';
// #3217 — split the embed host: SEARCH query-embed stays on OLLAMA_URL (localhost,
// latency-critical, same box as chorus-api); BULK embed-delta uses OLLAMA_BULK_URL
// (Bedroom GPU offload, DEC-054). One shared var routed search cross-machine and a
// dead Bedroom host took search down for hours (2026-06-04). Defaults to OLLAMA_URL
// when unset (no behavior change without the env). Both set in the TRACKED
// chorus-env-setup.sh so a plist regen can't revert them.
// #3382 — EMBED_MODEL moved to the search worker (it owns query-embed now).
// Prefer repo scripts (always present), fall back to ~/.chorus/scripts
const REPO_ROOT = path.resolve(__dirname, '../../..');
const REPO_SCRIPTS = path.resolve(__dirname, '../../scripts');
const HOME_SCRIPTS = path.join(os.homedir(), '.chorus', 'scripts');
const SCRIPTS_DIR = fs.existsSync(REPO_SCRIPTS) ? REPO_SCRIPTS : HOME_SCRIPTS;

// --- Board card cache (#2096, extracted to src/board-cache.ts in #2205) ---
// Logic + types + hermetic tests live in src/board-cache.ts. Server.ts
// retains only the runner wiring (which binary, which env, refresh cadence).

import { createBoardCache, CachedCard } from './board-cache';

const boardCache = createBoardCache({
  run: async () => {
    const boardTs = path.join(REPO_SCRIPTS, 'cards');
    const envOpts = {
      encoding: 'utf-8' as const, timeout: 15000,
      env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
    };
    const { stdout } = await execAsync(`bash ${boardTs} list 2>/dev/null`, envOpts);
    return stdout;
  },
  // #3610 — op attribution scoped INSIDE refresh to the sync parse slice only.
  // The old wrap here set currentOp before the async refresh and cleared it in
  // .finally(), so the label spanned the whole cards-list shell-out await and
  // ANY block by ANY code during an in-flight refresh was reported as
  // op=boardCache — the mislabel that tainted the nightly trace evidence.
  setOp: setCurrentOp,
});

const getBoardCards = (): CachedCard[] => boardCache.getCards();

void boardCache.refresh();
// .unref() so this background refresh timer never keeps the process alive on
// its own — in prod the HTTP listener holds the loop; in tests (which import
// server.ts via startTestApp) an un-unref'd interval kept the jest worker alive
// → "worker failed to exit gracefully" force-exit → the pipeline read a passing
// suite as tests:fail. The recurring flake. The timer still fires every 60s.
setInterval(() => {
  void boardCache.refresh();
}, 60_000).unref();

// --- LanceDB semantic search — OFF-PROCESS (#3382) ---
// The lance handle + searchInTable moved out of chorus-api entirely. lance's
// native CPU pool (lance-cpu) is process-global and does continuous background
// work on the open handle; in-process it starved the event loop (the
// 2026-06-12/13 wedge storm: 7,327 eventloop.blocked events, 24 kickstarts each
// good for ~30min). The handle now lives in dist/search-worker.js — a forked
// CHILD PROCESS, not a worker_thread (a thread shares the process, so it would
// NOT isolate the native pool). chorus-api dispatches via the shared worker pool
// and never opens or scans lance again. The worker inherits CHORUS_LANCE_DIR, so
// a variant chorus-api (#3381) forks its own worker at its own dir.
// (fork imported as `forkChild` in the consolidated child_process import at top — #3429)
import { createWorkerPool, type WorkerLike } from './worker-pool';
import type { SearchRequest } from './search-worker-core';

const SEARCH_WORKER_SCRIPT = process.env.CHORUS_SEARCH_WORKER_SCRIPT
  || path.join(__dirname, 'search-worker.js');

function spawnSearchWorker(): WorkerLike {
  const cp = forkChild(SEARCH_WORKER_SCRIPT, [], { env: process.env });
  cp.on('error', (err) => console.error(`[chorus-api] search worker spawn error: ${err.message}`));
  return {
    postMessage: (m) => { cp.send(m as object); },
    on: (event, cb) => { cp.on(event, cb as (...a: unknown[]) => void); },
    terminate: () => cp.kill(),
  };
}

const searchPool = createWorkerPool<{ query: string; limit: number; role?: string; op?: 'search' | 'count' }, SearchRequest>({
  spawn: spawnSearchWorker,
  label: 'search',
  buildRequest: (id, q) => ({ id, query: q.query, limit: q.limit, role: q.role, op: q.op }),
});

// --- Embed-at-ingest: embed new messages after indexing ---

// #3379 — embed-delta NO LONGER RUNS IN-PROCESS. The pass interleaves
// synchronous better-sqlite3 page reads with lance writes; on this event loop
// it wedged the whole API (2026-06-12: 5 outages, convicted by isolation —
// 2.6% CPU calm with the pass off, 65-100% wedges with it on). The wiring
// moved whole to embed-delta-deps.ts (the #3085 index-all-sources-deps
// pattern) and runs in dist/embed-delta-worker.js via chorus-embed-worker.sh.
// On-demand triggers below SPAWN the worker detached; they never run the pass.
// (#3214's single-flight concern moved with it: the launcher's lockfile is the
// cross-process single-flight; in-process coalescing lives on in the deps.)
const EMBED_WORKER_SCRIPT = process.env.CHORUS_EMBED_WORKER_SCRIPT
  || path.join(CHORUS_ROOT, 'platform/scripts/chorus-embed-worker.sh');
const REINDEX_WORKER_SCRIPT = process.env.CHORUS_REINDEX_WORKER_SCRIPT
  || path.join(CHORUS_ROOT, 'platform/scripts/chorus-reindex-worker.sh');
function spawnDetachedWorker(script: string): void {
  const child = spawn('/bin/bash', [script], { detached: true, stdio: 'ignore' });
  // Kade's #3379 gather catch: a 202 that spawns nothing is a false-green
  // sibling. Without this listener an ENOENT 'error' event is also an
  // UNHANDLED EventEmitter error — it would crash the API. Log loud instead.
  child.on('error', (err) => {
    console.error(`[spawn-worker] FAILED to spawn ${script}: ${err.message}`);
  });
  child.unref();
}

// Embed query helper (extracted to src/embed-query.ts in #2205 wave 2).
// Retry + LRU cache + TTL live there; server.ts wires the Ollama URL + model.

// #3382 — query-embed moved into the off-process search worker (it owns the
// full embed→vector-search path now); chorus-api no longer embeds for search.
// #3217's bulk embedder (Bedroom GPU) moved to embed-delta-deps.ts with the
// pass itself (#3379) — OLLAMA_BULK_URL is read there from the same env.

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
  // #3382 — dispatched to the off-process search worker; returns [] when lance
  // is absent (the worker's null-table path), so callers no longer gate on it.
  return (await searchPool.run({ query, limit, role })) as SemanticResult[];
}
// STALE_THRESHOLD_MS moved to src/search-meta.ts (#2205 wave 5).
const FUSEKI_URL = process.env.FUSEKI_URL || 'http://localhost:3030/pods/query';

// --- SPARQL text search ---
// Extracted to src/sparql-search.ts (#2205 wave 4).
import { createSparqlSearch } from './sparql-search';

const sparqlSearch = createSparqlSearch({ fusekiUrl: FUSEKI_URL });

// --- Unified search: merge all sources via RRF ---
// RRF fusion + types moved to src/search-fusion.ts (#2205 wave 3).
import { mergeUnified, enrichHit, resolveSearchLimit } from './search-fusion';

// --- Spine event emitter (fire-and-forget to chorus-log.sh) ---
const CHORUS_LOG = path.join(CHORUS_ROOT, 'platform/scripts/chorus-log');
const OPS_NUDGE = path.join(CHORUS_ROOT, 'platform/scripts/ops-nudge');

// emitSearchEvent + getDb + DbNotFoundError moved to src/server-helpers.ts (#2205 wave 12).
import {
  createDbOpener,
  createSearchEventEmitter,
  createAlertFilesReader,
  crashAlert,
} from './server-helpers';
const getDb = createDbOpener<Database.Database>({
  dbPath: DB_PATH,
  exists: (p) => fs.existsSync(p),
  DatabaseCtor: Database,
});
const emitSearchEvent = createSearchEventEmitter({
  chorusLogPath: CHORUS_LOG,
  execFileFn: execFile as unknown as Parameters<typeof createSearchEventEmitter>[0]['execFileFn'],
});

// Staleness middleware + search meta extracted to src/search-meta.ts (#2205 wave 5).
import { addStaleHeader, buildSearchMeta, SOURCE_CADENCE } from './search-meta';

// enrichHit + resolveSearchLimit + SEARCH_* constants moved to search-fusion.ts.
// (enrichHit + resolveSearchLimit imported at line 241.)

// --- GET /api/chorus/search ---
// Supports mode=fts (default), mode=semantic, mode=hybrid

import { fetchSearch } from './handlers/chorus-search';
import { createWithDb } from './with-db';
import { Worker } from 'node:worker_threads';
import { createFtsPool } from './fts-worker-pool';
const withDb = createWithDb<Database.Database>(() => getDb());

// #3086 — FTS runs OFF the serving event loop in a worker_threads pool. Active only
// when the built worker exists (dist/fts-worker.js): prod uses the pool; ts-run tests
// without a build fall back to the in-process sync query. worker_threads inherits
// chorus-api's node, so the better-sqlite3 ABI mismatch that bit #3085 can't recur.
const ftsWorkerPath = path.join(__dirname, 'fts-worker.js');
const ftsPool = fs.existsSync(ftsWorkerPath)
  ? createFtsPool({ spawn: () => new Worker(ftsWorkerPath) })
  : null;

app.get('/api/chorus/search', async (req: Request, res: Response) => {
  await withDb(res, async (db) => {
    addStaleHeader(res, db);
    const r = await fetchSearch(
      {
        db,
        semanticSearch: semanticSearch as unknown as import('./handlers/chorus-search').SemanticSearchFn, // #3382: always available; worker returns [] if lance absent
        sparqlSearch: sparqlSearch as unknown as import('./handlers/chorus-search').SparqlSearchFn,
        mergeUnified: mergeUnified as unknown as import('./handlers/chorus-search').MergeUnifiedFn,
        mergeRRF: mergeRRF as unknown as import('./handlers/chorus-search').MergeRRFFn,
        emitSearchEvent,
        buildSearchMeta,
        enrichHit,
        resolveSearchLimit,
        ...(ftsPool
          ? { ftsSearch: (q: string, fetchLimit: number, role: string | undefined, mode: string) => ftsPool.runFtsAsync({ q, fetchLimit, role, mode }) }
          : {}),
      },
      {
        q: req.query.q as string | undefined,
        limit: req.query.limit as string | undefined,
        role: req.query.role as string | undefined,
        mode: req.query.mode as string | undefined,
      },
    );
    res.status(r.status).json(r.body);
  });
});

// --- GET /api/chorus/conversation ---
// Returns a readable conversation thread between participants in a time range.
// Memory domain — team recall, not search. #1946

import { fetchChorusConversation } from './handlers/chorus-conversation';
app.get('/api/chorus/conversation', async (req: Request, res: Response) => {
  await withDb(res, (db) => {
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
  });
});

// --- GET /api/chorus/card-story/:id ---
// Memory domain — join six data sources into a card timeline. #1947

import { fetchChorusCardStory, type CardMeta, type NudgeMessage } from './handlers/chorus-card-story';
import { safeReadFile, readFileTail } from './lib/log-reader';

// #3406 — tail budget for the /context/spine log read. 4MB holds ~8x MAX_LIMIT(500)
// recent JSONL spine events; reading only this (vs the full ~535MB log) keeps the
// synchronous read off the multi-second freeze path.
const SPINE_TAIL_BYTES = 4 * 1024 * 1024;
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
        readLog: () => safeReadFile(logPath),
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
app.get('/api/chorus/domain-story/:domain', (req: Request, res: Response) => {
  const logPath = path.resolve(__dirname, '../../logs/chorus.log');
  let db: Database.Database | null = null;
  try { db = getDb(); } catch { /* db optional */ }
  try {
    const r = fetchChorusDomainStory(
      {
        getCards: () => getBoardCards(),
        db,
        readLog: () => safeReadFile(logPath),
      },
      req.params.domain,
      req.query.limit as string | undefined,
    );
    res.status(r.status).json(r.body);
  } finally {
    if (db) db.close();
  }
});

// --- GET /api/chorus/crawl/:domain (#1956, extracted #2189) ---
import { fetchCrawl } from './handlers/chorus-crawl';
app.get('/api/chorus/crawl/:domain', async (req: Request, res: Response) => {
  let db: Database.Database | null = null;
  try { db = getDb(); } catch { /* db optional */ }
  try {
    const r = await fetchCrawl(req.params.domain, {
      db,
      getBoardCards,
      fetchFn: fetch as unknown as import('./handlers/chorus-crawl').FetchFn,
      athenaSparqlQuery,
      execAsync,
      readFile: (p, enc) => fs.readFileSync(p, enc),
      exists: (p) => fs.existsSync(p),
      readdir: (p) => fs.readdirSync(p),
      chorusLogPath: path.resolve(__dirname, '../../logs/chorus.log'),
      memoryDir: path.join(os.homedir(), '.claude/projects/-Users-jeffbridwell-CascadeProjects/memory'),
      // shared-observability is a sibling of the chorus checkout (#3197 —
      // CHORUS_ROOT is now always the chorus dir, so the path is unambiguous).
      alertDir: path.join(CHORUS_ROOT, '..', 'shared-observability/config/grafana/provisioning/alerting'),
    });
    res.status(r.status).json(r.body);
  } finally { if (db) db.close(); }
});

// #3373: /api/chorus/domain/:domain/code-files RETIRED (deprecated by #2060,
// zero non-test consumers confirmed by grep). /code is the surviving facet.
// Retirement gate: platform/tests/3373-code-files-retired.bats.

// --- Consolidated domain facet API (#2060) ---
// One endpoint per facet under /api/chorus/domain/:name/.
// AX = UX: same shape whether rendering for Jeff or briefing a role on /pull.

// resolveSubdomainId + isTestFile moved to src/subdomain-resolver.ts (#2205 wave 10).
// The sparql dep is lazy-bound — athenaSparqlQuery is declared further down
// in the module (post-#2205 wave 8), so an eager capture would TDZ here.
import { createSubdomainResolver } from './subdomain-resolver';
const resolveSubdomainId = createSubdomainResolver({ sparql: (q: string) => athenaSparqlQuery(q) });

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

// readAlertFiles moved to src/server-helpers.ts (#2205 wave 12).
// (createAlertFilesReader + crashAlert imported at line 247.)
const readAlertFiles = createAlertFilesReader({
  fs,
  alertsDir: path.join(REPO_ROOT, 'proving/domains/alerts'),
});
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
  // #3039 — was execSync('git log', UNBOUNDED full history) on the event loop:
  // a cold route that froze the WHOLE process for up to its 10s timeout whenever
  // git contended on the shared repo (the measured 6.24s idle stall). Now read
  // async (off the loop) and bounded (-n 300 — deploy history needs recent acps,
  // not the entire history). The handler stays sync; we hand it the pre-fetched
  // string, so no handler/test ripple.
  let gitLogOut = '';
  try {
    const { stdout } = await execAsync('git log -n 300 --format="%H|%aI|%s"', {
      cwd: REPO_ROOT, encoding: 'utf-8', timeout: 10000,
    });
    gitLogOut = stdout;
  } catch {
    gitLogOut = ''; // handler treats empty as zero releases (same as the old catch)
  }
  const r = fetchChorusDomainReleases(
    { gitLog: () => gitLogOut, getCards: getBoardCards, envelope: athenaEnvelope },
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
// Time utilities moved to src/time-utils.ts (#2205 wave 6).
import { isEDT, bostonNow, convertToLocal } from './time-utils';

/** Reciprocal Rank Fusion — merge FTS + semantic results by message ID */
// #2168 AC-14: query-aware RRF weighting extracted to ./search-rrf.ts
// so it can be unit-tested without pulling the whole server module.
import { hasExactToken, mergeRRF } from './search-rrf';
export { hasExactToken, mergeRRF };

// --- GET /api/chorus/reconcile ---

import { fetchChorusReconcile } from './handlers/chorus-reconcile';
app.get('/api/chorus/reconcile', async (req: Request, res: Response) => {
  await withDb(res, (db) => {
    addStaleHeader(res, db);
    const r = fetchChorusReconcile({ db }, { role: req.query.role as string | undefined });
    res.status(r.status).json(r.body);
  });
});

// --- GET /api/chorus/refs ---

import { fetchChorusRefs } from './handlers/chorus-refs';
app.get('/api/chorus/refs', async (req: Request, res: Response) => {
  await withDb(res, (db) => {
    addStaleHeader(res, db);
    const r = fetchChorusRefs({ db }, {
      card: req.query.card as string | undefined,
      wf: req.query.wf as string | undefined,
      type: req.query.type as string | undefined,
      entityId: req.query.id as string | undefined,
    });
    res.status(r.status).json(r.body);
  });
});

// --- GET /api/chorus/stats ---

import { fetchChorusStats } from './handlers/chorus-stats';
app.get('/api/chorus/stats', async (_req: Request, res: Response) => {
  await withDb(res, (db) => {
    addStaleHeader(res, db);
    const r = fetchChorusStats({ db });
    res.status(r.status).json(r.body);
  });
});

// --- GET /api/chorus/freshness (#1879) ---
// SOURCE_CADENCE moved to src/search-meta.ts (#2205 wave 5) — imported at line 264.

import { fetchFreshness } from './handlers/chorus-freshness';
import { fetchContextFreshness } from './handlers/context-freshness';
import { createFreshnessCache } from './freshness-cache';

function runFreshnessHandler() {
  if (!fs.existsSync(DB_PATH)) {
    return { status: 503, body: { error: 'Index database not found' } };
  }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    return fetchFreshness({
      db,
      exists: (p) => fs.existsSync(p),
      spineLogPath: `${process.env.HOME}/.chorus/chorus.log`,
      cadence: SOURCE_CADENCE,
      timestamp: bostonNow,
    });
  } finally {
    db.close();
  }
}

// #3060 - fetchFreshness reads the 170MB spine log + COUNTs ~838K spine rows,
// ~1.4s of synchronous work that blocked the event loop (the coordination spine)
// on every poll. The cache serves the last snapshot on the request path in <1ms
// and recomputes off the request tick (stale-while-revalidate), so no request
// blocks the loop. TTL 30s — well inside the hourly+ source cadences. The
// recompute's own residual sync cost moving fully off-loop (worker thread) is
// the structural follow-on #3055.
const freshnessCache = createFreshnessCache(runFreshnessHandler, { ttlMs: 30_000 });
freshnessCache.get(); // pre-warm at boot so the first live request never pays the cold cost

// New canonical path under /api/chorus/context/* (#2252).
app.get('/api/chorus/context/freshness', async (req: Request, res: Response) => {
  const r = await fetchContextFreshness(
    { sparql: _athena, runFreshness: () => freshnessCache.get() },
    req.originalUrl,
  );
  res.status(r.status).json(r.body);
});

// Legacy path stays live (returns same JSON) while callers migrate to
// /api/chorus/context/freshness. 301 redirect + telemetry wave follows
// once callers (test suites, bats) are carded — see #2252 follow-on.
app.get('/api/chorus/freshness', (_req: Request, res: Response) => {
  const r = freshnessCache.get();
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/pulse/latest (#1881) ---
// Returns most recent Pulse team state snapshot

import { fetchChorusPulseLatest } from './handlers/chorus-pulse-latest';

// #3202 — read the durable pulse snapshot (~/.chorus, survives reboot) FIRST;
// fall back to the /tmp derived cache during transition. The durable file is the
// single source of truth; /tmp is a cache the Rust producer mirrors. After a
// reboot wipes /tmp, the durable last-good still serves — no blind boot.
const readPulseSnapshot = (): string | null => {
  const durable = process.env.CHORUS_PULSE_PATH
    || `${process.env.HOME || ''}/.chorus/pulse-latest.json`;
  if (durable && fs.existsSync(durable)) return fs.readFileSync(durable, 'utf-8');
  if (fs.existsSync('/tmp/pulse-latest.json')) return fs.readFileSync('/tmp/pulse-latest.json', 'utf-8');
  return null;
};

app.get('/api/chorus/pulse/latest', (_req: Request, res: Response) => {
  const r = fetchChorusPulseLatest({ readPulse: () => readPulseSnapshot() });
  res.status(r.status).json(r.body);
});

// --- Context API (#2234 Step 3) ---
// Pull-model endpoints with common envelope + Athena-graph-stamped headers.
// Three endpoints for the proof-of-shape: board/wip, roles, health.

import { fetchContextBoardWip } from './handlers/context-board-wip';
import { fetchContextSpine } from './handlers/context-spine';
import { fetchContextAlerts } from './handlers/context-alerts';
import { fetchContextQualitySummary } from './handlers/context-quality-summary';
import { fetchContextBoardNext } from './handlers/context-board-next';
import { fetchContextCoverage } from './handlers/context-coverage';
import { fetchContextBoardSwat } from './handlers/context-board-swat';
import { fetchContextRoles } from './handlers/context-roles';
import { fetchContextHealth } from './handlers/context-health';

const readPulseFile = (): string | null => readPulseSnapshot();

const readRoleStateFile = (role: string): { role: string; state: string; card?: number | null; gemba?: string | null; detail?: string | null } | null => {
  const p = `/tmp/claude-team-scan/${role}-declared.json`;
  try {
    if (!fs.existsSync(p)) return null;
    const parsed = JSON.parse(fs.readFileSync(p, 'utf-8'));
    return {
      role,
      state: typeof parsed.state === 'string' ? parsed.state : 'unknown',
      card: typeof parsed.card === 'number' ? parsed.card : null,
      gemba: typeof parsed.gemba === 'string' ? parsed.gemba : null,
      detail: typeof parsed.detail === 'string' ? parsed.detail : null,
    };
  } catch {
    return null;
  }
};

const tailSpineForRole = (role: string): { timestamp: string; role: string; event: string } | null => {
  // Producer is chorus-hook-shim writing to ~/.chorus/chorus.log (CSC: Runtime
  // Artifacts). 2026-05-04: moved out of repo working tree because branch
  // checkouts were clobbering unstaged writes.
  const candidates = [
    `${process.env.HOME}/.chorus/chorus.log`,
  ];
  const logPath = candidates.find((p) => fs.existsSync(p));
  if (!logPath) return null;
  try {
    if (!fs.existsSync(logPath)) return null;
    // Read the tail; spine log is append-only JSONL. Reading last 64KB is
    // enough to find the most recent per-role event without scanning the whole file.
    const stat = fs.statSync(logPath);
    const start = Math.max(0, stat.size - 64 * 1024);
    const fd = fs.openSync(logPath, 'r');
    const buf = Buffer.alloc(stat.size - start);
    fs.readSync(fd, buf, 0, buf.length, start);
    fs.closeSync(fd);
    const lines = buf.toString('utf-8').split('\n').filter(Boolean);
    for (let i = lines.length - 1; i >= 0; i--) {
      try {
        const parsed = JSON.parse(lines[i]);
        if (parsed.role === role && typeof parsed.event === 'string') {
          return {
            timestamp: typeof parsed.timestamp === 'string' ? parsed.timestamp : '',
            role,
            event: parsed.event,
          };
        }
      } catch { /* skip malformed */ }
    }
  } catch { /* best effort */ }
  return null;
};

app.get('/api/chorus/context/alerts', async (req: Request, res: Response) => {
  const alertDir = [
    `${CHORUS_ROOT}/proving/domains/alerts`,
  ].find((p) => fs.existsSync(p));
  const r = await fetchContextAlerts(
    {
      sparql: _athena,
      readPulse: readPulseFile,
      listAlertFiles: () => {
        try { return alertDir ? fs.readdirSync(alertDir) : []; } catch { return []; }
      },
      readAlertFile: (name) => {
        if (!alertDir) return null;
        try { return fs.readFileSync(path.join(alertDir, name), 'utf-8'); } catch { return null; }
      },
    },
    req.originalUrl,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/spine', async (req: Request, res: Response) => {
  const limit = typeof req.query.limit === 'string' ? req.query.limit : undefined;
  // #3406 — bound the spine read to its tail. This endpoint used to fs.readFileSync
  // the WHOLE ~535MB chorus.log on every request, synchronously blocking the event
  // loop 5-8s — the chronic freeze root (profiler-proven). 4MB holds ~8x the
  // MAX_LIMIT(500) recent JSONL events; parseTailEvents scans from the end and
  // skips the partial leading line, so the recent slice is identical for ~0 cost.
  const readLog = () => readFileTail(`${process.env.HOME}/.chorus/chorus.log`, SPINE_TAIL_BYTES);
  const r = await fetchContextSpine({ sparql: _athena, readLog }, req.originalUrl, limit);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/board/wip', async (req: Request, res: Response) => {
  const roleFilter = typeof req.query.role === 'string' ? req.query.role : undefined;
  const r = await fetchContextBoardWip(
    { sparql: _athena, readPulse: readPulseFile },
    req.originalUrl,
    roleFilter,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/coverage', async (req: Request, res: Response) => {
  const domain = typeof req.query.domain === 'string' ? req.query.domain : undefined;
  const r = await fetchContextCoverage(
    {
      sparql: _athena,
      fetchDomainFiles: async (d: string) => {
        const domainSuffix = d.endsWith('-domain') || d.endsWith('-service') ? d : `${d}-domain`;
        const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domainSuffix}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
        try {
          const result = await athenaSparqlQuery(query);
          return result.results.bindings.map((b: SparqlBinding) => b.filePath.value as string);
        } catch {
          return [];
        }
      },
      readCoverageSummary: () => {
        const candidates = [
          `${CHORUS_ROOT}/platform/api/coverage/coverage-summary.json`,
        ];
        const p = candidates.find((c) => fs.existsSync(c));
        if (!p) return null;
        try { return fs.readFileSync(p, 'utf-8'); } catch { return null; }
      },
    },
    req.originalUrl,
    domain,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/board/next', async (req: Request, res: Response) => {
  const roleFilter = typeof req.query.role === 'string' ? req.query.role : undefined;
  const r = await fetchContextBoardNext(
    { sparql: _athena, readPulse: readPulseFile },
    req.originalUrl,
    roleFilter,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/board/swat', async (req: Request, res: Response) => {
  const roleFilter = typeof req.query.role === 'string' ? req.query.role : undefined;
  const r = await fetchContextBoardSwat(
    { sparql: _athena, readPulse: readPulseFile },
    req.originalUrl,
    roleFilter,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/roles', async (req: Request, res: Response) => {
  const r = await fetchContextRoles(
    {
      sparql: _athena,
      readState: readRoleStateFile,
      tailSpine: tailSpineForRole,
      // #2193 AC5: inferred state from derive-role-state
      readInferred: (role: string) => {
        const p = `/tmp/claude-team-scan/${role}-inferred.json`;
        try {
          if (!fs.existsSync(p)) return null;
          return JSON.parse(fs.readFileSync(p, 'utf-8'));
        } catch { return null; }
      },
    },
    req.originalUrl,
  );
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/context/health', async (req: Request, res: Response) => {
  const r = await fetchContextHealth(
    { sparql: _athena, readPulse: readPulseFile },
    req.originalUrl,
  );
  res.status(r.status).json(r.body);
});

// #2652 AC9 — POST /api/cards/<verb> mutation routes.
// Thin HTTP wrappers that spawn the cards bash CLI as subprocess with
// DEPLOY_ROLE injected from X-Role header. Same canonical chain as bash CLI
// and MCP tools. Subprocess contract per cards-service-design.md:
//   - 10s timeout
//   - X-Role required (refuse 400 if missing)
//   - Exit codes: 0=success, 2=validation, 3=persistence, 4=network, 1=other
//   - HTTP status mapping: 0→200, 2→400, 3→502, 4→503, *→500
const cardsExecFileAsync = promisify(execFile);
const CARDS_BIN = path.resolve(__dirname, '..', '..', 'scripts', 'cards');

interface CardsExecResult { ok: boolean; stdout: string; stderr: string; code: number; }

async function runCardsCli(role: string, verb: string, args: string[]): Promise<CardsExecResult> {
  const env = { ...process.env, DEPLOY_ROLE: role, CHORUS_CARDS_ORIGIN: 'http' };
  try {
    const { stdout, stderr } = await cardsExecFileAsync(CARDS_BIN, [verb, ...args], { env, timeout: 10_000 });
    return { ok: true, stdout, stderr, code: 0 };
  } catch (err: unknown) {
    const e = err as { code?: number; stdout?: string; stderr?: string; message?: string };
    return {
      ok: false,
      stdout: e.stdout ?? '',
      stderr: e.stderr ?? e.message ?? String(err),
      code: typeof e.code === 'number' ? e.code : 1,
    };
  }
}

function httpStatusFromCardsExit(code: number): number {
  if (code === 0) return 200;
  if (code === 2) return 400;
  if (code === 3) return 502;
  if (code === 4) return 503;
  return 500;
}

function requireRoleHeader(req: Request, res: Response): string | null {
  const role = (req.header('X-Role') || req.header('x-role') || '').toLowerCase();
  if (!['wren', 'silas', 'kade', 'jeff', 'automation'].includes(role)) {
    res.status(400).json({ error: 'X-Role header required (wren|silas|kade|jeff|automation)' });
    return null;
  }
  return role;
}

app.post('/api/cards/add', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const b = req.body as { title?: string; owner?: string; priority?: string; domain?: string; type?: string; origin?: string; desc?: string; sequence?: string; chunk?: string; subdomain?: string; subproduct?: string };
  if (!b.title || !b.owner || !b.priority || !b.domain || !b.type || !b.origin || !b.desc) {
    res.status(400).json({ error: 'required: title, owner, priority, domain, type, origin, desc' });
    return;
  }
  const args = [b.title, '--owner', b.owner, '--priority', b.priority, '--domain', b.domain, '--type', b.type, '--origin', b.origin, '--desc', b.desc];
  if (b.sequence) args.push('--sequence', b.sequence);
  if (b.chunk) args.push('--chunk', b.chunk);
  if (b.subdomain) args.push('--subdomain', b.subdomain);
  if (b.subproduct) args.push('--subproduct', b.subproduct);
  const r = await runCardsCli(role, 'add', args);
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: r.ok, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

app.post('/api/cards/move', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const { id, status } = req.body as { id?: number; status?: string };
  if (!id || !status) { res.status(400).json({ error: 'required: id, status' }); return; }
  const r = await runCardsCli(role, 'move', [String(id), status]);
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: r.ok, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

app.post('/api/cards/done', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const { id } = req.body as { id?: number };
  if (!id) { res.status(400).json({ error: 'required: id' }); return; }
  const r = await runCardsCli(role, 'done', [String(id)]);
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: r.ok, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

app.post('/api/cards/tag', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const { id, category, value, op } = req.body as { id?: number; category?: string; value?: string; op?: string };
  if (!id || !category || !value) { res.status(400).json({ error: 'required: id, category, value' }); return; }
  let verb = 'tag', args: string[] = [String(id), `${category}:${value}`];
  if (category === 'sequence' && op !== 'remove') { verb = 'sequence-tag'; args = [String(id), value]; }
  else if (op === 'remove') { verb = 'untag'; }
  const r = await runCardsCli(role, verb, args);
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: r.ok, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

app.post('/api/cards/set', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const { id, fields } = req.body as { id?: number; fields?: Record<string, string> };
  if (!id || !fields || typeof fields !== 'object') { res.status(400).json({ error: 'required: id, fields {key:value}' }); return; }
  const args = [String(id), ...Object.entries(fields).map(([k, v]) => `${k}=${v}`)];
  const r = await runCardsCli(role, 'set', args);
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: r.ok, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

app.post('/api/cards/view', async (req: Request, res: Response) => {
  const role = requireRoleHeader(req, res); if (!role) return;
  const { id } = req.body as { id?: number };
  if (!id) { res.status(400).json({ error: 'required: id' }); return; }
  const r = await runCardsCli(role, 'view', [String(id), '--json']);
  if (r.ok) {
    try { res.status(200).json(JSON.parse(r.stdout)); return; }
    catch { res.status(200).json({ ok: true, raw: r.stdout }); return; }
  }
  res.status(httpStatusFromCardsExit(r.code)).json({ ok: false, stdout: r.stdout, stderr: r.stderr, code: r.code });
});

// --- POST /api/chorus/reindex (#1879) ---
// Trigger full re-index + re-embed without app restart

app.post('/api/chorus/reindex', (_req: Request, res: Response) => {
  // #3379 — never run index/embed on this loop; hand both to their workers.
  spawnDetachedWorker(REINDEX_WORKER_SCRIPT);
  spawnDetachedWorker(EMBED_WORKER_SCRIPT);
  res.status(202).json({ status: 'spawned', workers: ['reindex', 'embed'], timestamp: bostonNow() });
});

// --- POST /api/chorus/index ---
// Inline indexing — replaces deleted bash scripts (#1879)

app.post('/api/chorus/index', (_req: Request, res: Response) => {
  // #3379 — off-loop; the embed cadence (com.chorus.embed-worker) follows up.
  spawnDetachedWorker(REINDEX_WORKER_SCRIPT);
  res.status(202).json({ status: 'spawned', workers: ['reindex'], timestamp: bostonNow() });
});

// indexAllSources left server.ts entirely in #3379 — the worker
// (index-worker.ts via index-all-sources-deps.ts) is the only runner; the
// on-demand routes above spawn it detached instead of importing it.


// --- GET /api/chorus/self — Read-only filtered endpoint for Self (DEC-068) ---
// Source whitelist: memory, story, decision, brief, adr
// Blocks: claude (raw sessions), spine (ops events), slack, clearing, activity, state

const SELF_SOURCE_WHITELIST = new Set(['memory', 'story', 'decision', 'brief', 'adr']);

import { fetchSelf } from './handlers/chorus-self';
app.get('/api/chorus/self', async (req: Request, res: Response) => {
  await withDb(res, async (db) => {
    const r = await fetchSelf(
      {
        db,
        semanticSearch: semanticSearch as unknown as import('./handlers/chorus-self').SemanticSearchFn, // #3382: always available; worker returns [] if lance absent
        sparqlSearch: sparqlSearch as unknown as import('./handlers/chorus-self').SparqlSearchFn,
        mergeUnified: mergeUnified as unknown as import('./handlers/chorus-self').MergeUnifiedFn,
        emitSearchEvent,
        whitelist: SELF_SOURCE_WHITELIST,
      },
      { q: req.query.q as string | undefined, limit: req.query.limit as string | undefined },
    );
    res.status(r.status).json(r.body);
  });
});

// --- POST /api/chorus/embed (trigger embed-delta on demand) ---

app.post('/api/chorus/embed', (_req: Request, res: Response) => {
  // #3379 — the pass runs in dist/embed-delta-worker.js, never here.
  spawnDetachedWorker(EMBED_WORKER_SCRIPT);
  res.status(202).json({ status: 'spawned', workers: ['embed'], timestamp: bostonNow() });
});

// --- POST /api/chorus/pulse (spine event emission — replaces chorus-log.sh) ---

// Lifecycle-write handlers (pulse / role-state / alert) moved to
// src/lifecycle-writes.ts (#2205 wave 19).
import { handlePulse, handleRoleState, handleAlert } from './lifecycle-writes';
const LIFECYCLE_LOG = `${process.env.HOME}/.chorus/chorus.log`;
app.post('/api/chorus/pulse', (req: Request, res: Response) => {
  handlePulse(req, res, {
    appendFileSync: fs.appendFileSync,
    chorusLogPath: LIFECYCLE_LOG,
    now: bostonNow,
  });
});

// --- POST /api/chorus/role-state (replaces role-state.sh) ---

app.post('/api/chorus/role-state', (req: Request, res: Response) => {
  handleRoleState(req, res, {
    appendFileSync: fs.appendFileSync,
    writeFileSync: fs.writeFileSync,
    chorusLogPath: LIFECYCLE_LOG,
  });
});

// --- POST /api/chorus/alert (Grafana webhook receiver) ---

app.post('/api/chorus/alert', (req: Request, res: Response) => {
  handleAlert(req, res, {
    appendFileSync: fs.appendFileSync,
    notify: (title, message) => {
      execFile(
        'osascript',
        ['-e', `display notification "${message.replace(/"/g, '\\"')}" with title "${title.replace(/"/g, '\\"')}" sound name "Basso"`],
        (err) => { if (err) console.error('Notification failed:', err.message); },
      );
    },
    chorusLogPath: LIFECYCLE_LOG,
  });
});

// --- GET /api/chorus/voice-analytics ---

import { fetchChorusVoiceAnalytics } from './handlers/chorus-voice-analytics';
app.get('/api/chorus/voice-analytics', async (req: Request, res: Response) => {
  await withDb(res, (db) => {
    addStaleHeader(res, db);
    const r = fetchChorusVoiceAnalytics({ db, isEDT }, { days: req.query.days as string | undefined });
    res.status(r.status).json(r.body);
  });
});

// --- GET /api/chorus/reprompt-analytics ---

import { fetchChorusRepromptAnalytics } from './handlers/chorus-reprompt-analytics';
app.get('/api/chorus/reprompt-analytics', async (req: Request, res: Response) => {
  await withDb(res, (db) => {
    addStaleHeader(res, db);
    const r = fetchChorusRepromptAnalytics({ db }, { days: req.query.days as string | undefined });
    res.status(r.status).json(r.body);
  });
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

  execFile('bash', [scriptPath, role, audioPath], { timeout: 30000 }, (err, _stdout, stderr) => {
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
import { fetchContextPerf } from './handlers/context-perf';
app.get('/api/chorus/perf', async (_req: Request, res: Response) => {
  const r = await fetchPerf({ scriptPath: PERF_SCRIPT });
  res.status(r.status).json(r.body);
});

// New canonical path under /api/chorus/context/* (#2252 migration).
app.get('/api/chorus/context/perf', async (req: Request, res: Response) => {
  const r = await fetchContextPerf(
    { sparql: _athena, runPerf: () => fetchPerf({ scriptPath: PERF_SCRIPT }) },
    req.originalUrl,
  );
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

const COST_SCRIPT = path.join(CHORUS_ROOT, 'platform/scripts/cost-report.sh');

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

// #3039 — Prometheus scrape target. Was 404; the most load-bearing process had
// zero latency visibility. Exposes nodejs_eventloop_lag_seconds so a blocked
// loop is measured, not inferred. #2482's tool-call counters share this registry.
app.get('/metrics', async (_req: Request, res: Response) => {
  const { contentType, body } = await getMetrics();
  res.set('Content-Type', contentType);
  res.send(body);
});

// --- ICD Write API (#1549) — mirrors app write endpoints, no auth ---

const FUSEKI_UPDATE_URL = process.env.FUSEKI_UPDATE_URL || 'http://localhost:3030/pods/update';
const FUSEKI_QUERY_URL = process.env.FUSEKI_QUERY_URL || 'http://localhost:3030/pods/sparql';
const ICD_GRAPH = 'https://jeffbridwell.com/icd/current';
const ICD_PFX = 'PREFIX icd: <https://jeffbridwell.com/icd#>';

// escSparql + icdSlug moved to src/sparql-helpers.ts (#2205 wave 7).
import { escSparql, icdSlug } from './sparql-helpers';

// ICD SPARQL client + domain resolver moved to src/icd-sparql.ts (#2205 wave 9).
import { createIcdSparqlClient, createIcdDomainResolver, fusekiWriteAuthFromEnv } from './icd-sparql';
const _icd = createIcdSparqlClient({ queryUrl: FUSEKI_QUERY_URL, updateUrl: FUSEKI_UPDATE_URL, auth: fusekiWriteAuthFromEnv() });
const icdSparqlQuery = _icd.query;
const icdSparqlUpdate = _icd.update;
const resolveIcdDomain = createIcdDomainResolver({ client: _icd, pfx: ICD_PFX, graph: ICD_GRAPH });

// POST /api/icd/domains/:id/fields
// ICD field upsert handler moved to src/icd-writes.ts (#2205 wave 22).
import { handleIcdFieldUpsert, handleIcdMappingUpsert, handleIcdSectionPut } from './icd-writes';
app.post('/api/icd/domains/:id/fields', async (req: Request, res: Response) => {
  await handleIcdFieldUpsert(req, res, {
    resolveDomain: resolveIcdDomain,
    client: { query: icdSparqlQuery, update: icdSparqlUpdate },
    pfx: ICD_PFX, graph: ICD_GRAPH,
    icdSlug, escSparql,
  });
});

// POST /api/icd/domains/:id/mappings
// (handleIcdMappingUpsert + handleIcdSectionPut imported at line 1041.)
const icdDeps = () => ({
  resolveDomain: resolveIcdDomain,
  client: { query: icdSparqlQuery, update: icdSparqlUpdate },
  pfx: ICD_PFX, graph: ICD_GRAPH,
  icdSlug, escSparql,
});
app.post('/api/icd/domains/:id/mappings', async (req: Request, res: Response) => {
  await handleIcdMappingUpsert(req, res, icdDeps());
});

// PUT /api/icd/domains/:id/providers/:pid/sections
app.put('/api/icd/domains/:id/providers/:pid/sections', async (req: Request, res: Response) => {
  await handleIcdSectionPut(req, res, icdDeps());
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
const domainResponseCache = new Map<string, { body: unknown; ts: number }>();
const DOMAIN_CACHE_TTL_MS = 60 * 1000;

import { fetchChorusDomain, Completeness } from './handlers/chorus-domain';
app.get('/api/chorus/domain/:name', async (_req: Request, res: Response) => {
  const name = _req.params.name.toLowerCase();
  const cached = domainResponseCache.get(name);
  if (cached && Date.now() - cached.ts < DOMAIN_CACHE_TTL_MS) {
    res.json(cached.body);
    return;
  }
  try {
    const r = await fetchChorusDomain(
      {
        domainRegistry: DOMAIN_REGISTRY,
        getCards: getBoardCards,
        readDomainHtml: (d: string) => {
          const p = `${CHORUS_ROOT}/platform/roles/product-manager/artifacts/domain-${d}.html`;
          return safeReadFile(p);
        },
        fetchCompleteness: async (sdId: string) => {
          try {
            const resp = await fetch(`http://localhost:3340/api/athena/subdomains/${sdId}/completeness`);
            if (!resp.ok) return null;
            const body = await resp.json() as { data?: Completeness };
            return body.data ?? null;
          } catch { return null; }
        },
        sparql: athenaSparqlQuery,
      },
      _req.params.name,
    );
    if (r.status === 200) domainResponseCache.set(name, { body: r.body, ts: Date.now() });
    res.status(r.status).json(r.body);
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

// Health cache moved to src/health-cache.ts (#2205 wave 13).
import { createHealthCache } from './health-cache';
const _healthCache = createHealthCache({
  dbPath: DB_PATH,
  DatabaseCtor: Database,
  // #3382 moved the lance handle off-process; the stub here made /health/detail
  // report vectors:0 forever (test-api-health red + lancedb-stale false alert
  // while 13GB of vectors sat on disk). #3606: a facade whose countRows rides
  // the worker protocol (op:'count'), so the health cache code is unchanged.
  getLanceTable: () => ({
    countRows: async () => {
      const rows = (await searchPool.run({ query: '', limit: 0, op: 'count' })) as Array<{ count?: number }>;
      return rows[0]?.count ?? 0;
    },
  }),
  fs: { existsSync: (p) => fs.existsSync(p), statSync: (p) => fs.statSync(p) },
  hookBinaryPath: path.resolve(__dirname, '../../services/chorus-hooks/target/release/chorus-hooks'),
});
const refreshHealthCache = () => _healthCache.refresh();
// Legacy export for existing handler deps — returns the live snapshot object.
const healthCache = _healthCache.snapshot();

// Scheduled reindex moved to a standalone worker (chorus-reindex-worker.sh) — #3085.
// indexAllSources() uses synchronous better-sqlite3, so running it on the API event
// loop (the old in-process 15-min setInterval) blocked every request during a pass —
// the eventloop.blocked alerts. The #1605 "SQLite-only, safe in-process" assumption
// was falsified by #3079's live capture. The worker runs the same indexAllSources in
// its own process on the same 15-min cadence. POST /api/chorus/reindex still works for
// on-demand manual runs. (#3080 Track A / ADR-034: decouple compute via the store.)

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
import { fetchHealthDetail } from './handlers/chorus-health-detail';
app.get('/api/chorus/health/detail', async (_req: Request, res: Response) => {
  const r = await fetchHealthDetail({ healthCache, ollamaUrl: OLLAMA_URL, timestamp: bostonNow });
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/hooks/friction (#3280) ---
// Ranked friction view: which hook blocks whom, how often, per role, inside a
// sliding window (?hours=N, default 12). Reads hooks.log directly — no store.

import { fetchHookFriction } from './handlers/hook-friction';
app.get('/api/chorus/hooks/friction', (req: Request, res: Response) => {
  const HOOKS_LOG = path.join(os.homedir(), 'Library/Logs/Gathering/hooks.log');
  // #3282 — judgment-as-code: committed classification rules, joined at read.
  const CLASSES = path.join(CHORUS_ROOT, 'config/hook-friction-classes.json');
  const hours = Number(req.query.hours);
  const r = fetchHookFriction({
    readLog: () => safeReadFile(HOOKS_LOG),
    readClasses: () => safeReadFile(CLASSES),
    windowHours: Number.isFinite(hours) && hours > 0 && hours <= 24 * 14 ? hours : undefined,
  });
  res.status(r.status).json(r.body);
});

// --- GET /api/chorus/hooks/metrics (#2277) ---

let hooksMetricsCache: { data: unknown; ts: number } | null = null;
const HOOKS_CACHE_TTL = 60_000; // 60s

import { fetchChorusHooksMetrics } from './handlers/chorus-hooks-metrics';
app.get('/api/chorus/hooks/metrics', (_req: Request, res: Response) => {
  if (hooksMetricsCache && (Date.now() - hooksMetricsCache.ts) < HOOKS_CACHE_TTL) {
    res.json(hooksMetricsCache.data);
    return;
  }
  const HOOKS_LOG = path.join(os.homedir(), 'Library/Logs/Gathering/hooks.log');
  const r = fetchChorusHooksMetrics({
    readLog: () => safeReadFile(HOOKS_LOG),
  });
  if (r.status === 200) hooksMetricsCache = { data: r.body, ts: Date.now() };
  res.status(r.status).json(r.body);
});

// --- Crash handlers: log + alert before dying ---

// crashAlert moved to src/server-helpers.ts (#2205 wave 12); imported above.

// ── owl-api same-origin proxy (#3644) ────────────────────────────
// The Athena page family fetches the generated model API. Fetching
// `hostname:3360` directly couples every page to LAN topology and breaks the
// moment a page is served through any other origin (the share tunnel exposed
// this: browsers asked share-host:3360, which nothing serves). GET-only proxy;
// upstream overridable for tests.
app.use('/owl', async (req: Request, res: Response) => {
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    return res.status(405).json({ error: 'owl proxy is read-only (GET/HEAD)' });
  }
  // read at request time so tests can stub the upstream after import
  const owlUpstream = process.env.OWL_UPSTREAM || 'http://127.0.0.1:3360';
  try {
    const r = await fetch(owlUpstream + req.originalUrl.replace(/^\/owl/, ''), {
      headers: { Accept: String(req.headers.accept || 'application/json') },
    });
    res.status(r.status);
    const ct = r.headers.get('content-type');
    if (ct) res.type(ct);
    res.send(Buffer.from(await r.arrayBuffer()));
  } catch {
    res.status(502).json({ error: 'owl-api unreachable through the proxy' });
  }
});

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

// #2431 stopgap: domain-detail.html (served by gathering on :3000) fetches
// /api/loom/* and /api/chorus/domain/:id/decisions from :3340 for
// instance-body enrichment. Need CORS until Athena relocates per #2041 —
// remove this block once Athena is hosted alongside its consumers.
// Origin narrowed to localhost:3000 + reads only (per silas gate:arch hold
// #2431): chorus-api binds 0.0.0.0, '*' + write methods would open blanket
// CSRF surface. The legitimate consumer is gathering reads; nothing
// cross-origin writes here.
// #2041: remove once Athena relocates
app.use(['/api/loom', '/api/chorus'], (_req: Request, res: Response, next: NextFunction) => {
  res.header('Access-Control-Allow-Origin', 'http://localhost:3000');
  res.header('Access-Control-Allow-Methods', 'GET, OPTIONS');
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
  // #3603: products/subproducts retired from the hand-coded surface — owl-api
  // :3360/products (generated from chorus:ProductShape) is the product API.
  { name: 'subdomains', path: '/api/athena/subdomains', description: 'List sub-domains with owner, step. Filter: ?owner, ?step' },
  { name: 'blast-radius', path: '/api/athena/subdomains/:id/blast-radius', description: 'Which sub-products consume a given sub-domain' },
  { name: 'steps', path: '/api/athena/steps', description: 'Value stream steps with sub-domains at each step' },
  { name: 'owners', path: '/api/athena/owners', description: 'Owners with sub-domain counts' },
  { name: 'machines', path: '/api/athena/machines', description: 'Machines with running services' },
];

// Athena SPARQL client + envelope + loader extracted to
// src/athena-sparql.ts (#2205 wave 8).
const ATHENA_UPDATE = 'http://localhost:3030/pods/update';
import { createAthenaSparqlClient, createEnvelopeBuilder, createSparqlLoader } from './athena-sparql';
const _athena = createAthenaSparqlClient({ sparqlUrl: ATHENA_SPARQL, updateUrl: ATHENA_UPDATE, auth: fusekiWriteAuthFromEnv() });
const athenaSparqlQuery = _athena.query;
const athenaSparqlUpdate = _athena.update;
const athenaEnvelope = createEnvelopeBuilder({ graph: ATHENA_GRAPH, now: bostonNow });

// #3618 — load the secured-surface table from the model at boot. Populates the
// mutable ref the envelope middleware (mounted above) reads per-request. Fail
// LOUD on error (spine event + stderr) but do NOT crash the hub: an empty table
// gates nothing, so a transient Fuseki blip at boot degrades to open-and-flagged,
// never to a dead chorus-api. A health probe can catch the flagged state.
projectSecuredSurfaces({ sparql: (q: string) => athenaSparqlQuery(q) })
  .then((table) => {
    SECURED_SURFACES = table;
    execFile(CHORUS_LOG, ['security.envelope.loaded', 'silas', `surfaces=${table.length}`], () => {});
  })
  .catch((err: unknown) => {
    const msg = err instanceof Error ? err.message : String(err);
    process.stderr.write(`[security-envelope] surface-table load FAILED (gating nothing until reload): ${msg}\n`);
    execFile(CHORUS_LOG, ['security.envelope.load_failed', 'silas', `error=${msg.slice(0, 120)}`], () => {});
  });
const loadSparql = createSparqlLoader({ fs, sparqlDir: SPARQL_DIR });

// GET /api/athena/health — discovery endpoint, lists available queries
// Extracted to handlers/athena-health.ts (#2173 AC4). SPARQL client + query
// loader are injected so unit tests run without Fuseki.
import { fetchAthenaHealth } from './handlers/athena-health';
import { fetchAthenaValidate } from './handlers/athena-validate';
import { fetchAthenaSteps } from './handlers/athena-steps';
import { fetchAthenaOwners } from './handlers/athena-owners';
import { fetchAthenaMachines } from './handlers/athena-machines';
import { fetchLoomPolicies } from './handlers/loom-policies';
import { fetchLoomPrinciples } from './handlers/loom-principles';
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

// #3603: GET /api/athena/products RETIRED — owl-api :3360/products serves products.

// #2940 — Athena Move 0 tree endpoints. Same Zod-validated source as
// chorus_tree_get / chorus_ownership_lookup / chorus_blast_radius MCP tools.
import { loadTree, lookupOwnership, computeBlastRadius } from './handlers/athena-tree';
app.get('/api/athena/tree', (_req: Request, res: Response) => {
  try {
    const tree = loadTree();
    res.status(200).json(tree);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code === 'ENOENT' ? 404 : 422;
    res.status(code).json({ ok: false, reason: code === 404 ? 'tree-not-found' : 'schema-violation', error: (err as Error).message });
  }
});
app.get('/api/athena/ownership/:iri', (req: Request, res: Response) => {
  try {
    const r = lookupOwnership(loadTree(), req.params.iri);
    if (!r) return res.status(404).json({ ok: false, reason: 'not-found', iri: req.params.iri });
    res.status(200).json(r);
  } catch (err) {
    res.status(422).json({ ok: false, reason: 'schema-violation', error: (err as Error).message });
  }
});
app.get('/api/athena/blast-radius/:iri', (req: Request, res: Response) => {
  try {
    const r = computeBlastRadius(loadTree(), req.params.iri);
    if (!r) return res.status(404).json({ ok: false, reason: 'not-found', iri: req.params.iri });
    res.status(200).json(r);
  } catch (err) {
    res.status(422).json({ ok: false, reason: 'schema-violation', error: (err as Error).message });
  }
});

// #3603: GET /api/chorus/products + GET /api/athena/subproducts RETIRED —
// SubProduct is gone from the model; owl-api :3360/products (generated from
// chorus:ProductShape) is the only product serving surface.

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

// GET /api/loom/principles — 308 redirect to Athena (#2314 — Loom GET retired post-ADR-025).
// New canonical path: /api/athena/subdomains/loom-principles/principles. Frontend updated to fetch
// the canonical path directly; this redirect serves any remaining callers.
app.get('/api/loom/principles', (_req: Request, res: Response) => {
  res.redirect(308, '/api/athena/subdomains/loom-principles/principles');
});

// GET /api/loom/decisions — 308 redirect to Athena (#2485 Move 2, mirror of principles).
app.get('/api/loom/decisions', (_req: Request, res: Response) => {
  res.redirect(308, '/api/athena/subdomains/loom-decisions/decisions');
});

// GET /api/athena/subdomains/:id/principles — principles inside a SubDomain (#2314).
// Currently scoped to loom-principles; reuses the existing principle folding logic
// (parent set, sort, envelope) from handlers/loom-principles.ts.
app.get('/api/athena/subdomains/:id/principles', async (_req: Request, res: Response) => {
  const r = await fetchLoomPrinciples({ sparql: athenaSparqlQuery, loadQuery: loadSparql });
  res.status(r.status).json(r.body);
});

// GET /api/loom/policies — all chorus:Policy instances + enforces edges (#2339)
app.get('/api/loom/policies', async (_req: Request, res: Response) => {
  const r = await fetchLoomPolicies({ sparql: athenaSparqlQuery, loadQuery: loadSparql });
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/cards — active board cards for this domain
app.get('/api/athena/subdomains/:id/cards', (req: Request, res: Response) => {
  const r = fetchAthenaSubdomainCards(
    { getBoardCards, envelope: athenaEnvelope },
    req.params.id,
  );
  res.status(r.status).json(r.body);
});

// GET /api/athena/subdomains/:id/alerts — alert rules related to this domain
app.get('/api/athena/subdomains/:id/alerts', (req: Request, res: Response) => {
  const ALERTS_DIR = path.join(REPO_ROOT, 'proving/domains/alerts');
  const r = fetchAthenaSubdomainAlerts(
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
    const fileId = `${req.params.id}-code-${name.replace(/[/.]/g, '-').toLowerCase()}`;
    const fileUri = `https://jeffbridwell.com/chorus#${fileId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${fileUri}> a chorus:CodeFile ; rdfs:label "${name.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasCodeFile <${fileUri}> . ${filePath ? `<${fileUri}> chorus:filePath "${filePath.replace(/"/g, '\\"')}" .` : ''} ${fileType ? `<${fileUri}> chorus:fileType "${fileType}" .` : ''} ${description ? `<${fileUri}> rdfs:comment "${description.replace(/"/g, '\\"')}" .` : ''} } }`;
    await athenaSparqlUpdate(update);
    res.json(athenaEnvelope('subdomain-code-create', { subdomain: req.params.id, uri: fileUri, label: name, path: filePath || null, type: fileType || null, description: description || null }, Date.now() - start));
  } catch (err: unknown) { res.status(500).json(athenaEnvelope('subdomain-code-create', { error: errMsg(err) }, Date.now() - start, { error: true })); }
});

// POST /api/athena/discover-code — auto-discover code files per domain from filesystem (#1868 AC1)
// discover-code moved to src/discover-code.ts (#2205 wave 25).
import { createDiscoverCode } from './discover-code';
const _discoverCode = createDiscoverCode({
  sparqlClient: { query: (q: string) => athenaSparqlQuery(q), update: (u: string) => athenaSparqlUpdate(u) },
  fs, path,
  gatheringRoot: path.resolve(__dirname, '../../../../jeff-bridwell-personal-site'),
  chorusRoot: path.resolve(__dirname, '../../..'),
});
app.post('/api/athena/discover-code', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const data = await _discoverCode();
    res.json(athenaEnvelope('discover-code', data, Date.now() - start, { count: data.total_files }));
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('discover-code', { error: errMsg(err) }, Date.now() - start, { error: true }));
  }
});


// discover-tests moved to src/discover-tests.ts (#2205 wave 24).
import { createDiscoverTests } from './discover-tests';
import { scanLoomHtml } from './discover-pages-loom';
import { scanAthenaHtml } from './discover-pages-athena';
import { scanEjsViews, scanDocHtml, type PageEntry } from './discover-pages-gathering';
import { parseChorusApiRoutes } from './discover-endpoints-chorus-api';
const _discoverTests = createDiscoverTests({
  sparqlClient: { query: (q: string) => athenaSparqlQuery(q), update: (u: string) => athenaSparqlUpdate(u) },
  fs, path,
  gatheringRoot: path.resolve(__dirname, '../../../../jeff-bridwell-personal-site'),
  chorusRoot: path.resolve(__dirname, '../../..'),
});
app.post('/api/athena/discover-tests', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const data = await _discoverTests();
    res.json(athenaEnvelope('discover-tests', data, Date.now() - start, { count: data.total_tests }));
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('discover-tests', { error: errMsg(err) }, Date.now() - start, { error: true }));
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
const DISCOVER_PAGES_GENERIC_BASES = new Set(['services', 'service', 'domains', 'domain', 'code', 'loom', 'time', 'streams', 'stream', 'messages', 'message', 'policies', 'policy']);

// #2627: domain IDs hoisted to consts (each was duplicated 5-33x; the
// chorus-domain literal hit 33 occurrences, which is the textbook
// agent-inlining pattern the no-duplicate-string rule catches).
const D_BLOG = 'blog-domain';
const D_SOCIAL = 'social-domain';
const D_SEEDS = 'seeds-domain';
const D_CHORUS = 'chorus-domain';
const D_PROPERTY = 'property-domain';
const D_IDEAS = 'ideas-domain';
const D_DOCS = 'documents-domain';

const DISCOVER_PAGES_ALIAS_OVERRIDES: Record<string, string> = {
  blog: D_BLOG, wordpress: D_BLOG,
  social: D_SOCIAL, socialpost: D_SOCIAL,
  seed: D_SEEDS, seeds: D_SEEDS,
  'self-ai': 'sexuality-domain', ontology: 'convergence-domain',
  chorus: D_CHORUS, werk: D_CHORUS, flow: D_CHORUS,
  garden: D_PROPERTY, gardening: D_PROPERTY,
};

async function buildPageAliasMap(): Promise<Record<string, string>> {
  const sdQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }';
  const sdResult = await athenaSparqlQuery(sdQuery);
  const domains = sdResult.results.bindings.map((b: SparqlBinding) => ({
    id: b.sd.value.split('#').pop() as string,
  }));
  const aliasToId: Record<string, string> = {};
  for (const d of domains) {
    const base = d.id.replace(/-(domain|service)$/, '');
    if (DISCOVER_PAGES_GENERIC_BASES.has(base)) continue;
    aliasToId[base] = d.id;
    if (base.endsWith('s') && !base.endsWith('ss')) {
      if (base.endsWith('ies')) aliasToId[base.replace(/ies$/, 'y')] = d.id;
      else aliasToId[base.replace(/s$/, '')] = d.id;
    }
  }
  return { ...aliasToId, ...DISCOVER_PAGES_ALIAS_OVERRIDES };
}

app.post('/api/athena/discover-pages', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const aliasToId = await buildPageAliasMap();
    // #3097 — env-driven, not a hardcoded sibling path: chorus-api must not break
    // when gathering relocates or is absent (the readers below already guard
    // existsSync, so an absent root yields empty entries, never a throw).
    const GATHERING_ROOT = gatheringRepoRoot;
    // #2485 Move 6 — scan chorus/platform/api/public/loom/ for loom-* subdomain pages.
    const validSubdomainIds = new Set<string>(Object.values(aliasToId));
    const loomEntries = scanLoomHtml(path.join(REPO_ROOT, 'platform/api/public/loom'), validSubdomainIds);
    // #2041 — scan chorus/platform/api/public/athena/ for athena-domain pages.
    const athenaEntries = scanAthenaHtml(path.join(REPO_ROOT, 'platform/api/public/athena'), validSubdomainIds);
    const entries: PageEntry[] = [
      ...scanEjsViews(path.join(GATHERING_ROOT, 'views'), aliasToId),
      ...scanDocHtml(path.join(GATHERING_ROOT, 'public/gathering-docs'), aliasToId),
      ...loomEntries,
      ...athenaEntries,
    ];

    // 4. Clear existing page data and repopulate
    const clearQuery = 'DELETE WHERE { GRAPH <urn:chorus:instances> { ?p a <https://jeffbridwell.com/chorus#Page> ; ?prop ?val . ?sd <https://jeffbridwell.com/chorus#hasPage> ?p . } }';
    await athenaSparqlUpdate(clearQuery);

    // 5. Write to graph in batches
    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < entries.length; i += batchSize) {
      const batch = entries.slice(i, i + batchSize);
      const triples = batch.map(e => {
        const pageId = `page-${e.path.replace(/[/.]/g, '-').toLowerCase()}`;
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('discover-pages', { error: errMsg(err) }, Date.now() - start, { error: true }));
  }
});

// GET /api/athena/subdomains/:id/pages — pages for a domain (#2065)
app.get('/api/athena/subdomains/:id/pages', async (req: Request, res: Response) => {
  const r = await fetchAthenaSubdomainPages({ sparql: athenaSparqlQuery, envelope: athenaEnvelope }, req.params.id);
  res.status(r.status).json(r.body);
});

type EndpointEntry = { method: string; path: string; handler: string; domainId: string };

const DISCOVER_ENDPOINTS_HANDLER_OVERRIDES: Record<string, string> = {
  bookHandler: 'books-domain', bookUploadHandler: 'books-domain',
  seedHandler: D_SEEDS, socialpostHandler: D_SOCIAL,
  personHandler: 'people-domain', collectionHandler: D_BLOG,
  glimmerHandler: 'glimmers-domain', ideaProjectHandler: D_IDEAS,
  codebaseGraphHandler: D_CHORUS, dashboardHandler: D_CHORUS,
  flowHandler: D_CHORUS, werkHandler: D_CHORUS,
  ontologyViewHandler: 'convergence-domain', galleryHandler: 'gallery-domain',
  gardenHandler: D_PROPERTY, icdHandler: 'convergence-domain',
  docCatalogHandler: D_DOCS, docsHandler: D_DOCS,
  documentHandler: D_DOCS, accessDashboardHandler: D_CHORUS,
  aclHandler: D_CHORUS, sessionReplayHandler: D_CHORUS,
  staticPageHandler: D_CHORUS, linkInferenceHandler: 'knowledge-domain',
  knowledgeGraphHandler: 'knowledge-domain', selfDomainHandler: 'self-domain',
  selfAiHandler: 'sexuality-domain', sexualityHandler: 'sexuality-domain',
  cookingHandler: 'cooking-domain', fitnessFunctionsHandler: D_CHORUS,
  intentionHandler: D_IDEAS, notesHandler: 'notes-domain',
  noteHandler: 'notes-domain', readingHandler: 'reading-domain',
  storiesHandler: 'stories-domain', storyHandler: 'stories-domain',
  watchingHandler: 'watching-domain', todoHandler: D_IDEAS,
  groupHandler: 'people-domain', qualityHandler: D_CHORUS,
  rolesHandler: 'roles-domain', skillsHandler: 'skills-service',
  teamHandler: D_CHORUS, briefsHandler: D_CHORUS,
  cardsHandler: 'cards-service', costHandler: D_CHORUS,
  hooksHandler: D_CHORUS, decisionsHandler: D_CHORUS,
  gardeningHandler: D_PROPERTY, webhookHandler: D_SEEDS,
  userHandler: D_CHORUS, aboutHandler: D_CHORUS,
  aboutProfileHandler: D_CHORUS, homeHandler: D_CHORUS,
  loginHandler: D_CHORUS, callbackHandler: D_CHORUS,
  profileHandler: D_CHORUS, logoutHandler: D_CHORUS,
};

const DISCOVER_ENDPOINTS_ROUTE_PREFIXES: Record<string, string> = {
  '/api/books': 'books-domain', '/books': 'books-domain',
  '/api/music': 'music-domain', '/music': 'music-domain',
  '/api/photos': 'photos-domain', '/photos': 'photos-domain',
  '/api/property': D_PROPERTY, '/property': D_PROPERTY,
  '/api/seed': D_SEEDS,
  '/api/glimmers': 'glimmers-domain',
  '/api/ideas': D_IDEAS,
  '/api/collections': D_BLOG, '/blog': D_BLOG,
  '/api/search': 'search-domain', '/search': 'search-domain',
  '/api/gallery': 'gallery-domain', '/gallery': 'gallery-domain',
  '/api/documents': D_DOCS, '/documents': D_DOCS,
  '/api/codebase': D_CHORUS,
  '/api/dashboard': D_CHORUS, '/dashboard': D_CHORUS,
  '/api/admin': D_CHORUS,
  '/api/icd': 'convergence-domain',
  '/api/chorus': D_CHORUS,
  '/api/athena': D_CHORUS,
  '/cooking': 'cooking-domain', '/notes': 'notes-domain',
  '/reading': 'reading-domain', '/stories': 'stories-domain',
  '/watching': 'watching-domain', '/todo': D_IDEAS,
  '/gardening': D_PROPERTY, '/people': 'people-domain',
  '/socialposts': D_SOCIAL, '/self': 'self-domain',
  '/sexuality': 'sexuality-domain', '/api/sessions': D_CHORUS,
  '/api/roles': 'roles-domain',
};

async function buildHandlerToDomain(): Promise<Record<string, string>> {
  const sdQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }';
  const sdResult = await athenaSparqlQuery(sdQuery);
  const domains = sdResult.results.bindings.map((b: SparqlBinding) => ({ id: b.sd.value.split('#').pop() as string }));
  const map: Record<string, string> = {};
  for (const d of domains) {
    const base = d.id.replace(/-(domain|service)$/, '');
    map[base + 'Handler'] = d.id;
    if (base.endsWith('s') && !base.endsWith('ss')) {
      const singular = base.endsWith('ies') ? base.replace(/ies$/, 'y') : base.replace(/s$/, '');
      map[singular + 'Handler'] = d.id;
    }
  }
  return { ...map, ...DISCOVER_ENDPOINTS_HANDLER_OVERRIDES };
}

function resolveEndpointDomain(handlerName: string | null, routePath: string, handlerToDomain: Record<string, string>): string | null {
  if (handlerName && handlerToDomain[handlerName]) return handlerToDomain[handlerName];
  for (const [prefix, did] of Object.entries(DISCOVER_ENDPOINTS_ROUTE_PREFIXES)) {
    if (routePath.startsWith(prefix)) return did;
  }
  return null;
}

function parseAppRoutes(appContent: string, handlerToDomain: Record<string, string>): EndpointEntry[] {
  const entries: EndpointEntry[] = [];
  const routeRegex = /app\.(get|post|put|delete|patch)\s*\(\s*['"`]([^'"`]+)['"`]/gi;
  let match;
  while ((match = routeRegex.exec(appContent)) !== null) {
    const method = match[1].toUpperCase();
    const routePath = match[2];
    const lineEnd = appContent.indexOf('\n', match.index);
    const lineContent = appContent.substring(match.index, lineEnd > 0 ? lineEnd : match.index + 200);
    const handlerMatch = lineContent.match(/(\w+Handler)\.\w+/);
    const handlerName = handlerMatch ? handlerMatch[1] : null;
    const domainId = resolveEndpointDomain(handlerName, routePath, handlerToDomain);
    if (!domainId) continue;
    entries.push({
      method,
      path: routePath,
      handler: handlerName ? `gathering/src/handlers/${handlerName.replace(/Handler$/, '')}.handler.ts` : 'gathering/src/app.ts',
      domainId,
    });
  }
  return entries;
}

async function writeEndpointsInBatches(entries: EndpointEntry[]): Promise<number> {
  const batchSize = 50;
  let written = 0;
  for (let i = 0; i < entries.length; i += batchSize) {
    const batch = entries.slice(i, i + batchSize);
    const triples = batch.map((e) => {
      const epId = `endpoint-${e.method.toLowerCase()}-${e.path.replace(/[/.:]/g, '-').toLowerCase()}`;
      const epUri = `https://jeffbridwell.com/chorus#${epId}`;
      const sdUri = `https://jeffbridwell.com/chorus#${e.domainId}`;
      return `<${epUri}> a chorus:Endpoint ; rdfs:label "${e.method} ${e.path.replace(/"/g, '\\"')}" ; chorus:httpMethod "${e.method}" ; chorus:routePath "${e.path.replace(/"/g, '\\"')}" ; chorus:filePath "${e.handler.replace(/"/g, '\\"')}" . <${sdUri}> chorus:hasEndpoint <${epUri}> .`;
    }).join('\n');
    const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
    await athenaSparqlUpdate(insert);
    written += batch.length;
  }
  return written;
}

app.post('/api/athena/discover-endpoints', async (_req: Request, res: Response) => {
  const start = Date.now();
  try {
    const handlerToDomain = await buildHandlerToDomain();
    // #3097 — env-driven, not a hardcoded sibling path (existsSync-guarded below).
    const GATHERING_ROOT = gatheringRepoRoot;
    const appTsPath = path.join(GATHERING_ROOT, 'src/app.ts');
    const gatheringEntries = fs.existsSync(appTsPath)
      ? parseAppRoutes(fs.readFileSync(appTsPath, 'utf-8'), handlerToDomain)
      : [];
    // #2485 Move 8 — also scan chorus-api's own server.ts so loom-*, chorus-domain
    // get hasEndpoint edges for the routes they actually own.
    const validSubdomainIds = new Set<string>(Object.values(handlerToDomain));
    const chorusApiSrc = path.join(REPO_ROOT, 'platform/api/src/server.ts');
    const chorusEntries = fs.existsSync(chorusApiSrc)
      ? parseChorusApiRoutes(fs.readFileSync(chorusApiSrc, 'utf-8'), validSubdomainIds)
      : [];
    const entries = [...gatheringEntries, ...chorusEntries];

    const clearQuery = 'DELETE WHERE { GRAPH <urn:chorus:instances> { ?ep a <https://jeffbridwell.com/chorus#Endpoint> ; ?p ?o . ?sd <https://jeffbridwell.com/chorus#hasEndpoint> ?ep . } }';
    await athenaSparqlUpdate(clearQuery);
    const written = await writeEndpointsInBatches(entries);

    const byDomain: Record<string, number> = {};
    for (const e of entries) byDomain[e.domainId] = (byDomain[e.domainId] || 0) + 1;

    res.json(athenaEnvelope('discover-endpoints', {
      total_endpoints: entries.length,
      total_domains: Object.keys(byDomain).length,
      by_domain: byDomain,
      entries,
      written,
    }, Date.now() - start, { count: entries.length }));
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('discover-endpoints', { error: errMsg(err) }, Date.now() - start, { error: true }));
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
app.post('/api/chorus/open', async (req: Request, res: Response) => {
  res.header('Access-Control-Allow-Origin', '*');
  const { path: filePath } = req.body || {};
  if (!filePath) return res.status(400).json({ error: 'Missing path' });
  const resolved = path.resolve(REPO_ROOT, filePath);
  if (!resolved.startsWith(REPO_ROOT)) return res.status(403).json({ error: 'Path outside repo' });
  try {
    // #3039 — was execSync('open "..."') on the event loop (a launch that can
    // block, and string-interpolated into a shell = injection risk). Now async
    // execFile with array args: off the loop AND no shell, so no injection.
    await cardsExecFileAsync('open', [resolved]);
    res.json({ ok: true, opened: resolved });
  } catch (err: unknown) {
    res.status(500).json({ error: errMsg(err) });
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
  createSubdomainPrinciple,
  updateSubdomainActor,
  updateSubdomainScenario,
  updateSubdomainContract,
  updateSubdomainPriorArt,
  updateSubdomainPrinciple,
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

// Envelope enrichment writes (#2206) — POST description + reads/writes/consumes edges.
// Pairs with #2208 data regression. Writes go to Fuseki via athenaSparqlUpdate AND
// append to a checked-in TTL seed so enrichment survives Fuseki rebuild.
import {
  fetchAthenaServiceDescription,
  fetchAthenaPersistenceDescription,
  fetchAthenaServiceEdge,
} from './handlers/athena-enrichment-write';
import { setSubdomainOwner } from './handlers/athena-owner-write';
// Seed lives in src/sparql/seeds/ — always version-controlled, never in dist.
// Resolve from ../src so this works whether server runs from src (ts-node/jest) or dist (compiled).
const ENRICHMENT_SEED_PATH = path.resolve(__dirname, '..', 'src', 'sparql', 'seeds', 'athena-enrichment.ttl');
const enrichmentDeps = () => ({
  sparqlUpdate: athenaSparqlUpdate,
  appendSeed: (triple: string) => {
    try {
      fs.appendFileSync(ENRICHMENT_SEED_PATH, triple + '\n');
    } catch (err) {
      console.error(`[enrichment] appendSeed failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  },
});

app.post('/api/athena/subdomains/:id/services/:eid/description', async (req: Request, res: Response) => {
  const r = await fetchAthenaServiceDescription(enrichmentDeps(), {
    subdomainId: req.params.id, entityId: req.params.eid, body: req.body || {},
  });
  res.status(r.status).json(r.body);
});

app.post('/api/athena/subdomains/:id/persistence/:eid/description', async (req: Request, res: Response) => {
  const r = await fetchAthenaPersistenceDescription(enrichmentDeps(), {
    subdomainId: req.params.id, entityId: req.params.eid, body: req.body || {},
  });
  res.status(r.status).json(r.body);
});

for (const pred of ['reads', 'writes', 'consumes'] as const) {
  app.post(`/api/athena/subdomains/:id/services/:eid/${pred}`, async (req: Request, res: Response) => {
    const r = await fetchAthenaServiceEdge(enrichmentDeps(), {
      subdomainId: req.params.id, entityId: req.params.eid, predicate: pred, body: req.body || {},
    });
    res.status(r.status).json(r.body);
  });
}

// POST /api/athena/subdomains/:id/owner — re-assign SubDomain owner (#2508)
const ONTOLOGY_TTL_PATH = path.join(REPO_ROOT, 'roles/silas/ontology/chorus.ttl');
const ownerWriteDeps = () => ({
  sparqlUpdate: athenaSparqlUpdate,
  readTtl: () => fs.readFileSync(ONTOLOGY_TTL_PATH, 'utf-8'),
  writeTtl: (content: string) => fs.writeFileSync(ONTOLOGY_TTL_PATH, content, 'utf-8'),
});
app.post('/api/athena/subdomains/:id/owner', async (req: Request, res: Response) => {
  const r = await setSubdomainOwner(ownerWriteDeps(), {
    subdomainId: req.params.id,
    body: req.body || {},
  });
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

// POST /api/athena/subdomains/:id/principles — add principle to subdomain (#2314)
app.post('/api/athena/subdomains/:id/principles', async (req: Request, res: Response) => {
  const r = await createSubdomainPrinciple(subdomainWriteDeps(), req.params.id, req.body);
  res.status(r.status).json(r.body);
});

// PUT /api/athena/subdomains/:id/principles/:entityId — update principle (#2314)
app.put('/api/athena/subdomains/:id/principles/:entityId', async (req: Request, res: Response) => {
  const r = await updateSubdomainPrinciple(subdomainWriteDeps(), req.params.id, req.params.entityId, req.body);
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('subdomain-create', { error: errMsg(err) }, Date.now() - start, { error: true }));
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('subdomain-update', { error: errMsg(err) }, Date.now() - start, { error: true }));
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('subdomain-consumes-add', { error: errMsg(err) }, Date.now() - start, { error: true }));
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('subdomain-consumes-remove', { error: errMsg(err) }, Date.now() - start, { error: true }));
  }
});

// POST /api/athena/reload — reload ontology from TTL into Fuseki
app.post('/api/athena/reload', async (_req: Request, res: Response) => {
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
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('reload', { error: errMsg(err) }, Date.now() - start, { error: true }));
  }
});

// #2627: helpers extracted from /api/athena/validate route.
//
// Source of truth: these CURIE→URI prefixes are the conventional Athena
// namespace bindings — `chorus:` maps to the chorus class/property URI base,
// `rdfs:`/`owl:`/`rdf:` are W3C standards. The first entry tracks our own
// ontology base; the rest are W3C-stable. Update if either: (a) the chorus
// URI base changes (rare — coordinated with #1772 namespace work), or (b)
// Athena consumers introduce a new convention prefix (e.g. `chorus-svc:`).
// Drift risk: low — these are stable conventions, but the map is a static
// shadow of Athena's binding, so a future namespace expansion in Athena
// won't auto-flow here. Per Silas arch-review on #2627.
const ATHENA_PREFIX_MAP: Record<string, string> = {
  'chorus:': 'https://jeffbridwell.com/chorus#',
  'rdfs:': 'http://www.w3.org/2000/01/rdf-schema#',
  'owl:': 'http://www.w3.org/2002/07/owl#',
  'rdf:': 'http://www.w3.org/1999/02/22-rdf-syntax-ns#',
};

function expandPredicate(pred: string): string {
  for (const [prefix, uri] of Object.entries(ATHENA_PREFIX_MAP)) {
    if (pred.startsWith(prefix)) return pred.replace(prefix, uri);
  }
  return pred;
}

async function predicateExists(pred: string): Promise<boolean> {
  const fullUri = expandPredicate(pred);
  const query = `ASK WHERE { GRAPH <${ATHENA_GRAPH}> { ?s <${fullUri}> ?o } }`;
  try {
    const result = await athenaSparqlQuery(query);
    return Boolean(result.boolean);
  } catch {
    return false;
  }
}

// POST /api/athena/validate — consumer declares predicate dependencies, API checks if they exist (#1356)
app.post('/api/athena/validate', async (req: Request, res: Response) => {
  const start = Date.now();
  try {
    const predicates: string[] = req.body?.predicates;
    if (!Array.isArray(predicates) || predicates.length === 0) {
      return res.status(400).json(athenaEnvelope('validate', { error: 'Body must include predicates: string[]' }, Date.now() - start, { error: true }));
    }
    const valid: string[] = [];
    const missing: string[] = [];
    for (const pred of predicates) {
      (await predicateExists(pred) ? valid : missing).push(pred);
    }
    res.json(athenaEnvelope('validate', { valid, missing, total: predicates.length, valid_count: valid.length, missing_count: missing.length }, Date.now() - start));
  } catch (err: unknown) {
    res.status(500).json(athenaEnvelope('validate', { error: errMsg(err) }, Date.now() - start, { error: true }));
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

// ensureRcaTable moved to src/db-schema.ts (#2205 wave 14).
import { createRcaTableEnsurer, createTraceTableEnsurer } from './db-schema';
const ensureRcaTable = createRcaTableEnsurer({ dbPath: RCA_DB_PATH, DatabaseCtor: Database });

// Lazy init on first use
let rcaTableReady = false;

// RCA + trace create handlers moved to src/diagnostic-writes.ts (#2205 wave 20).
import { handleRcaCreate, handleTraceCreate } from './diagnostic-writes';
app.post('/api/chorus/rca', (req: Request, res: Response) => {
  handleRcaCreate(req, res, {
    dbPath: RCA_DB_PATH, DatabaseCtor: Database,
    ensureTable: () => { if (!rcaTableReady) { ensureRcaTable(); rcaTableReady = true; } },
    appendFileSync: fs.appendFileSync,
    chorusLogPath: LIFECYCLE_LOG,
    now: bostonNow,
  });
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

// Spine event POST handler moved to src/spine-event-write.ts (#2205 wave 21).
import { handleSpineEvent } from './spine-event-write';
const SPINE_EVENT_LOG = `${process.env.HOME}/.chorus/chorus.log`;
app.post('/api/chorus/spine-event', (req: Request, res: Response) => {
  handleSpineEvent(req, res, {
    appendFileSync: fs.appendFileSync,
    chorusLogPath: SPINE_EVENT_LOG,
    now: bostonNow,
    traceDbPath: DB_PATH, DatabaseCtor: Database,
    ensureTraceTable: () => { if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; } },
  });
});

// --- Trace Envelope — #2097 (ADR-024) ---
// Common message envelope with hop-level tracing across four call stacks.
// Traces auto-populate domain integration maps.

// ensureTraceTable moved to src/db-schema.ts (#2205 wave 14).
const ensureTraceTable = createTraceTableEnsurer({ dbPath: DB_PATH, DatabaseCtor: Database });

let traceTableReady = false;

// POST /api/chorus/trace — record a hop
app.post('/api/chorus/trace', (req: Request, res: Response) => {
  handleTraceCreate(req, res, {
    dbPath: DB_PATH, DatabaseCtor: Database,
    ensureTable: () => { if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; } },
    now: bostonNow,
  });
});

// /api/chorus/trace/* — correlation-id hop chain + observed integrations (extracted #2189)
import { fetchTraceByCorrelation, fetchTraceIntegrations } from './handlers/chorus-trace';
app.get('/api/chorus/trace/:correlationId', async (req: Request, res: Response) => {
  if (!traceTableReady) { ensureTraceTable(); traceTableReady = true; }
  const db = new Database(DB_PATH, { readonly: true });
  db.pragma('journal_mode = WAL');
  try {
    // #3621 — fold spine events by trace_id (Loki via logsForTrace) so the
    // viewer shows the werk run, not just HTTP hops (the #3609 {hops:[]} gap).
    // Fold is OPT-IN (?events=1): the 7d Loki scan costs seconds, and the
    // legacy hop-consumers (#2100/#2102/#2103 integration suites, hop tooling)
    // need the instant hop-only path — their 5s budgets red-ed the first night
    // the fold was unconditional (07-07: npm:api 11/1923). trace.html asks for it.
    const wantFold = req.query.events === '1' || req.query.events === 'true';
    const r = await fetchTraceByCorrelation(req.params.correlationId, {
      db,
      ...(wantFold ? {
        // Tiered window (#3621 07-07): the flat 7d scan TIMED OUT on live prod
        // (loki-unreachable abort) — the viewer rendered 0 events for a trace
        // with 433. Fresh traces (the common case) resolve in the fast 1d tier;
        // only genuinely old traces pay the 7d scan.
        fetchSpineEvents: async (traceId: string) => {
          const fast = await logsForTrace({ trace_id: traceId, time_window: '1d' }, painLokiDeps);
          if (fast.ok && fast.events.length > 0) return fast.events as Array<Record<string, unknown>>;
          const wide = await logsForTrace({ trace_id: traceId, time_window: '7d' }, painLokiDeps);
          if (!wide.ok) throw new Error(`${wide.reason}: ${wide.detail ?? ''}`);
          return wide.events as Array<Record<string, unknown>>;
        },
      } : {}),
    });
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

// Doc catalog (#2445) — relocated from gathering. Lift-and-shift; gathering's
// /api/doc-catalog endpoints stay live until callers migrate.
import { listCatalog as docCatalogList, addDoc as docCatalogAdd, domainArtifacts as docCatalogDomain, linkArtifact as docCatalogLink, buildDocCatalog } from './handlers/doc-catalog';
import { inferTags, SUBPRODUCT_DOMAINS, GATHERING_SUBDOMAINS } from './handlers/doc-tagger';
import { detectDrift } from './handlers/doc-tag-drift';
import { buildHierarchyTree, type AthenaShape } from './handlers/doc-catalog-tree';
app.get('/api/doc-catalog', docCatalogList);
app.post('/api/doc-catalog/add', docCatalogAdd);
app.get('/api/doc-catalog/domain/:domain', docCatalogDomain);
app.post('/api/doc-catalog/link', docCatalogLink);

// #2627: helpers extracted from /api/doc-catalog/tags route.
type TaggedDocSummary = { href: string; source: string; title: string; tags: ReturnType<typeof inferTags> };
type DocTagCoverage = { byProduct: Record<string, number>; bySubproduct: Record<string, number>; withProduct: number; withSubdomain: number };

function summarizeCoverage(tagged: TaggedDocSummary[]): DocTagCoverage {
  const out: DocTagCoverage = { byProduct: {}, bySubproduct: {}, withProduct: 0, withSubdomain: 0 };
  for (const t of tagged) {
    if (t.tags.product) {
      out.byProduct[t.tags.product] = (out.byProduct[t.tags.product] || 0) + 1;
      out.withProduct++;
    }
    if (t.tags.subproduct) {
      out.bySubproduct[t.tags.subproduct] = (out.bySubproduct[t.tags.subproduct] || 0) + 1;
    }
    if (t.tags.subdomain) out.withSubdomain++;
  }
  return out;
}

async function fetchDriftAgainstAthena(tagged: TaggedDocSummary[]): Promise<ReturnType<typeof detectDrift>> {
  try {
    const r = await fetch('http://localhost:3340/api/athena/subdomains?limit=100');
    const d = await r.json() as { data?: Array<{ id?: string }> };
    const valid = (d.data || []).map(x => x.id || '').filter(Boolean);
    return detectDrift(tagged, valid);
  } catch {
    return [];
  }
}

function readTagCoverageHistory(): Array<{ date: string; productPct: number; subdomainPct: number; drift: number }> {
  try {
    const historyPath = path.resolve(__dirname, '..', '..', '..', 'knowledge', 'doc-tag-coverage-history.tsv');
    if (!fs.existsSync(historyPath)) return [];
    const raw = fs.readFileSync(historyPath, 'utf-8');
    return raw.split('\n')
      .filter(line => line && !line.startsWith('#'))
      .map(line => {
        const [date, _total, _pt, productPct, _st, subdomainPct, driftCount] = line.split('\t');
        return {
          date,
          productPct: Number(productPct) || 0,
          subdomainPct: Number(subdomainPct) || 0,
          drift: Number(driftCount) || 0,
        };
      })
      .slice(-30);
  } catch {
    return [];
  }
}

function buildTaggedDocs(): TaggedDocSummary[] {
  const catalog = buildDocCatalog();
  const docs = catalog.groups.flatMap(g => g.docs);
  return docs.map(doc => {
    const basename = doc.href.split('/').pop() || '';
    const tags = inferTags({ sourcePath: `${doc.source}/${basename}`, basename });
    return { href: doc.href, source: doc.source, title: doc.title, tags };
  });
}

// Doc-tag coverage (#2520 AC4 + AC6) — applies inferTags + drift to live catalog
app.get('/api/doc-catalog/tags', async (_req: Request, res: Response) => {
  try {
    const tagged = buildTaggedDocs();
    const cov = summarizeCoverage(tagged);
    const drift = await fetchDriftAgainstAthena(tagged);
    const history = readTagCoverageHistory();
    res.json({
      total: tagged.length,
      coverage: {
        product: { tagged: cov.withProduct, percent: Math.round(100 * cov.withProduct / tagged.length) },
        subdomain: { tagged: cov.withSubdomain, percent: Math.round(100 * cov.withSubdomain / tagged.length) },
      },
      byProduct: cov.byProduct,
      bySubproduct: cov.bySubproduct,
      drift,
      tagged,
      history,
    });
  } catch (error) {
    res.status(500).json({ error: error instanceof Error ? error.message : String(error) });
  }
});

// Doc-catalog hierarchy tree (#2521) — Athena-driven navigation surface
app.get('/api/doc-catalog/tree', async (_req: Request, res: Response) => {
  try {
    const catalog = buildDocCatalog();
    const docs = catalog.groups.flatMap(g => g.docs);
    const tagged = docs.map(doc => {
      const basename = doc.href.split('/').pop() || '';
      const tags = inferTags({ sourcePath: `${doc.source}/${basename}`, basename });
      return { href: doc.href, source: doc.source, title: doc.title, tags };
    });

    // Compose Athena shape from owl-api (#3603: the hand-coded products/
    // subproducts endpoints are retired; owl-api :3360/products — generated from
    // chorus:ProductShape — is the product source). SubProduct is gone from the
    // model: the tree's middle level is the CHILD products (product-loom, …),
    // top level stays the hubs. SUBPRODUCT_DOMAINS remains the tagger bridge.
    const [pRes, sdRes] = await Promise.all([
      fetch('http://localhost:3360/products').then(r => r.json()),
      fetch('http://localhost:3340/api/athena/subdomains?limit=100').then(r => r.json()),
    ]) as [{ data?: Array<{ name?: string; label?: string; ownedBy?: string }> }, { data?: Array<{ id?: string; label?: string }> }];

    const TOP_LEVEL = new Set(['chorusProduct', 'gathering', 'borgProduct']);
    const allProducts = (pRes.data || []).map(p => ({
      id: p.name || '',
      label: p.label || '',
      owner: (p.ownedBy || '').replace(/^role-/, ''),
    }));
    // Hubs first (chorusProduct/gathering may predate serving; keep them present).
    const products = [
      { id: 'chorusProduct', label: 'Chorus' },
      { id: 'gathering', label: 'Gathering' },
      ...allProducts.filter(p => TOP_LEVEL.has(p.id)).map(({ id, label }) => ({ id, label })),
    ].filter((p, i, a) => a.findIndex(x => x.id === p.id) === i);

    // Middle level: child products (former subproduct slot in the tree shape).
    const subproducts = allProducts
      .filter(p => !TOP_LEVEL.has(p.id))
      .map(sp => ({ id: sp.id, label: sp.label, product: 'chorusProduct' }));

    // Build subdomain → child-product from SUBPRODUCT_DOMAINS (tagger short
    // names 'loom'/'werk'/… map to the product-* IRI convention; retired
    // 'quality' matches nothing and drops out).
    const sdToSp: Record<string, string> = {};
    for (const [sp, doms] of Object.entries(SUBPRODUCT_DOMAINS)) {
      for (const d of doms) {
        const match = subproducts.find(s => s.id === `product-${sp}` || s.id === sp || s.id === `${sp}-product`);
        if (match) sdToSp[d] = match.id;
      }
    }

    const subdomains = (sdRes.data || []).map(sd => {
      const id = sd.id || '';
      const subproduct = sdToSp[id] || null;
      const isGathering = GATHERING_SUBDOMAINS.has(id);
      return {
        id, label: sd.label || id,
        subproduct,
        product: !subproduct && isGathering ? 'gathering' : undefined,
      };
    });

    const shape: AthenaShape = { products, subproducts, subdomains };
    const tree = buildHierarchyTree(tagged, shape);
    res.json(tree);
  } catch (error) {
    res.status(500).json({ error: error instanceof Error ? error.message : String(error) });
  }
});

// Doc-catalog curation API (#2549) — five-field tag write + lineage edges.
// Tags persist in urn:chorus:instances; lineage edges use chorus:supersedes /
// chorus:derivedFrom / chorus:reshapedInto predicates. Spine events on every
// curation action.
import {
  writeCatalogTags,
  writeCatalogLineage,
  readCatalogDoc,
  readCatalogDrift,
  readCatalogCurated,
  readCatalogAudit,
} from './handlers/catalog-curation';

const catalogCurationDeps = () => ({
  sparqlQuery: athenaSparqlQuery,
  sparqlUpdate: athenaSparqlUpdate,
  envelope: athenaEnvelope,
  emitSpine: (event: string, fields: Record<string, string>) => {
    const args = [event, 'wren'];
    for (const [k, v] of Object.entries(fields)) args.push(`${k}=${v}`);
    execFile(CHORUS_LOG, args, () => { /* fire-and-forget */ });
  },
});

app.post('/api/chorus/catalog/tags', async (req: Request, res: Response) => {
  const r = await writeCatalogTags(catalogCurationDeps(), req.body);
  res.status(r.status).json(r.body);
});

app.post('/api/chorus/catalog/lineage', async (req: Request, res: Response) => {
  const r = await writeCatalogLineage(catalogCurationDeps(), req.body);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/catalog/doc/:hrefb64', async (req: Request, res: Response) => {
  const r = await readCatalogDoc(catalogCurationDeps(), req.params.hrefb64);
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/catalog/drift', async (_req: Request, res: Response) => {
  const r = await readCatalogDrift(catalogCurationDeps());
  res.status(r.status).json(r.body);
});

app.get('/api/chorus/catalog/curated', async (_req: Request, res: Response) => {
  // All CatalogDoc instances with their persisted tags. Used by doc-catalog.html
  // to merge curated overlay over the runtime tagger output so saves are visible.
  const r = await readCatalogCurated(catalogCurationDeps());
  res.status(r.status).json(r.body);
});

// Audit feed for a single doc — reads chorus.log directly (Loki ingestion
// of catalog.* events is unreliable today; file is source-of-truth).
const CHORUS_LOG_FILE = path.join(os.homedir(), '.chorus', 'chorus.log');
// #3406 — bound the catalog-audit read of the 535MB chorus.log to a tail.
const CATALOG_AUDIT_TAIL_BYTES = 16 * 1024 * 1024;

// #2627: extracted from inline arrow function (was cog=21 inside route).
type CatalogAuditEvent = { timestamp: string; event: string; role: string; fields: Record<string, string> };
const META_KEYS = new Set(['timestamp', 'event', 'role', 'level', 'appName', 'component']);

function asStrField(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

function isPrimitive(v: unknown): boolean {
  return typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean';
}

function parseCatalogLine(line: string, wantedHref: string): CatalogAuditEvent | null {
  if (!line || !line.includes('"catalog.')) return null;
  try {
    const obj = JSON.parse(line) as Record<string, unknown>;
    const ev = asStrField(obj.event);
    if (!ev.startsWith('catalog.')) return null;
    if (obj.href !== wantedHref) return null;
    const fields: Record<string, string> = {};
    for (const [k, v] of Object.entries(obj)) {
      if (!META_KEYS.has(k) && isPrimitive(v)) fields[k] = String(v);
    }
    return { timestamp: asStrField(obj.timestamp), event: ev, role: asStrField(obj.role), fields };
  } catch {
    return null;
  }
}

function readCatalogAuditEvents(href: string, n: number): Promise<CatalogAuditEvent[]> {
  // #3429 — sync body, but keep the Promise<> contract (the readEvents callback type
  // in readCatalogAudit expects it); return Promise.resolve rather than `async` (no await).
  const events: CatalogAuditEvent[] = [];
  // #3406 — was `fs.promises.readFile` of the whole 535MB chorus.log (async stat→read =
  // the AfterStat/ReadFileUtf8 OOM-crash stack the demo cold-eyes caught). We only scan
  // from the end for n href-matching events, so a bounded tail read is equivalent + safe.
  const data = readFileTail(CHORUS_LOG_FILE, CATALOG_AUDIT_TAIL_BYTES);
  if (data === null) return Promise.resolve(events);
  const lines = data.split('\n');
  for (let i = lines.length - 1; i >= 0 && events.length < n; i--) {
    const ev = parseCatalogLine(lines[i], href);
    if (ev) events.push(ev);
  }
  return Promise.resolve(events);
}

app.get('/api/chorus/catalog/audit/:hrefb64', async (req: Request, res: Response) => {
  const limitParam = typeof req.query.limit === 'string' ? req.query.limit : '20';
  const limit = Math.min(Number.parseInt(limitParam, 10) || 20, 200);
  const r = await readCatalogAudit({ readEvents: readCatalogAuditEvents, envelope: athenaEnvelope }, req.params.hrefb64, limit);
  res.status(r.status).json(r.body);
});

// Doc inventory (#2457) — reads TSV produced by doc-inventory.sh
app.get('/api/doc-inventory', (_req: Request, res: Response) => {
  const tsvPath = path.resolve(__dirname, '..', '..', '..', 'knowledge', 'doc-inventory.tsv');
  try {
    if (!fs.existsSync(tsvPath)) {
      res.status(404).json({ error: 'doc-inventory.tsv not found — run platform/scripts/doc-inventory.sh' });
      return;
    }
    const stat = fs.statSync(tsvPath);
    const raw = fs.readFileSync(tsvPath, 'utf-8').trim();
    // Schema (#2510 wave 4):
    //   repo  path  state  classification  owner  in-catalog  topic
    //   hash12  mtime  sha256
    // Header line starts with '#'; skip it.
    const rows = raw.split('\n')
      .filter(line => line && !line.startsWith('#'))
      .map(line => {
        const [repo, pathCol, state, classification, owner, inCatalog, topic, _hash, mtime, sha256] = line.split('\t');
        return {
          repo, path: pathCol, state,
          // Backward-compat: keep `cabinet` populated with classification
          cabinet: classification,
          classification,
          owner: owner || '',
          inCatalog, topic: topic || '',
          mtime: mtime || '', sha256: sha256 || '',
        };
      });
    const counts: Record<string, number> = {};
    const classCounts: Record<string, number> = {};
    for (const r of rows) {
      counts[r.state] = (counts[r.state] || 0) + 1;
      if (r.classification) classCounts[r.classification] = (classCounts[r.classification] || 0) + 1;
    }
    res.json({
      generatedAt: stat.mtime.toISOString(),
      total: rows.length,
      counts,
      classCounts,
      rows,
    });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    res.status(500).json({ error: msg });
  }
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
  console.log('[chorus-api] Received SIGTERM — shutting down');
  process.exit(0);
});

// Only bind + start background timers when run as the main module. Under jest
// (require.main !== module) tests import `app` and exercise routes in-process
// (#2167 landed the listen guard; #2173 AC4 moved the timers in here too —
// setInterval/setTimeout at module-load kept jest alive forever, masking
// that the harness worked at all).
const BIND_HOST = process.env.CHORUS_BIND || '0.0.0.0';
if (require.main === module) {
  // #3039 — start Node default-metric collection (event-loop lag, heap, GC) under
  // the live server. Idempotent; the /metrics route renders from the same registry.
  startMetrics();

  // #3050 — event-loop block alert via the `blocked` library (replaces the
  // hand-rolled shell alert). Real duration only, no fabricated story; the route is
  // correlated from the access log by timestamp. Cold-start excluded by the boot delay.
  // execFile (no shell) so the message can't shell-inject.
  // #3099: threshold 3000ms (was default 1000). chorus-api's real sync work —
  // search response marshaling, crawl parse slices post-#3091, board-poll
  // coincidences — regularly hits 1-2.5s; alerts on those are false positives,
  // not bugs. 3000ms surfaces real outliers (e.g., the 21s search tail-marshal
  // class) without firing on normal slow work. Drops ~80% of today's alert
  // volume. Revisit when CHORUS_API_PORT routing + worker-thread ALS land
  // (#3096 follow-on) — those eliminate the underlying overlap classes and
  // the threshold can come back down.
  // #3082: the in-process detector stays until the off-loop probe worker is verified
  // live (activate-before-retire); detector=in-process distinguishes its alerts from
  // the probe's (detector=probe) during the overlap, so the retirement is provable
  // from the data. This whole block is retired by the follow-on once the probe is proven.
  startEventloopAlert({
    emit: (a) =>
      execFile('bash', [CHORUS_LOG, 'eventloop.blocked', 'silas',
        'domain=chorus',
        `duration_ms=${a.duration_ms}`, `ts=${a.ts}`, `op=${a.op}`, 'detector=in-process',
        // #3610 — in stack-capture mode the first app frame is the measured call site
        ...(a.stack ? [`blocked_at=${firstAppFrame(a.stack)}`] : [])], () => {}),
    // #3407 — route the event-loop-block ALERT to wren (chorus-api is her layer);
    // spine-emit role above stays the chorus-api emitter context.
    nudge: (a) => execFile('bash', [OPS_NUDGE, 'wren', a.message], () => {}),
    threshold: 3000,
    // #3610 — bounded diagnostic window only (blocked-at async-hooks overhead is
    // NOT prod-default; #3050's decision stands). Set CHORUS_EVENTLOOP_STACKS=1
    // in the LaunchAgent env for a trace window, read the call sites, unset.
    captureStacks: process.env.CHORUS_EVENTLOOP_STACKS === '1',
  });

  // Health cache refresh — runs every 30s under the live server only.
  setTimeout(() => {
    setCurrentOp('healthCache');
    try { void refreshHealthCache(); } finally { setCurrentOp(null); }
  }, 2000);
  setInterval(() => {
    setCurrentOp('healthCache');
    try { void refreshHealthCache(); } finally { setCurrentOp(null); }
  }, 30_000);

  // Scheduled reindex runs in the standalone worker (chorus-reindex-worker.sh) — #3085.
  // No in-process timer here: reindex is synchronous SQLite and would block the loop.

  app.listen(PORT, BIND_HOST, () => {
    console.log(`[chorus-api] Listening on ${BIND_HOST}:${PORT}`);
    console.log(`[chorus-api] Database: ${DB_PATH}`);
    // #3382 — LanceDB no longer initialized in-process; the search worker opens
    // the handle in its own process, lazily, on the first semantic query.

    // Embed sync moved to standalone worker (chorus-embed-worker.sh) — #1978
    // The in-process timer was blocking the API with 100+ sequential Ollama calls per cycle.
    // POST /api/chorus/embed still works for on-demand batches.
  });
}

export default app;
