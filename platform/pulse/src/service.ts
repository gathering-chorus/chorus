/**
 * Messaging Tier Service — REST API for persistent messaging (#1755)
 *
 * Replaces /tmp file queues. SQLite-backed, queryable, observable.
 * CLI wrappers (nudge, chat.sh) call this API.
 */

import express, { Express } from 'express';
import { MessageStore } from './store';
import { DeliveryWorker, type RunInject, type EmitSpine, type SelfTest } from './delivery-worker';
import { planDelivery, resolveRoleTarget } from './session-registry';
import { dedupeKey, seenRecently } from './nudge-dedup';
import { Registry, Counter, Histogram, Gauge, collectDefaultMetrics } from 'prom-client';
import { spawn } from 'child_process';
import { appendFile } from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

const PORT = parseInt(process.env.MESSAGING_PORT || '3475');

// #3335 Pattern 7 — short-window dedup of identical concurrent nudges (from,to,content).
// A retry or double-fire within DEDUP_WINDOW_MS is dropped (returns ok, deduped:true);
// a legitimate re-send after the window goes through. Module-level, pruned on access.
const recentNudges = new Map<string, number>();
const DEDUP_WINDOW_MS = 10_000;

/**
 * Build an Express app bound to the given MessageStore. Factored out of the
 * top-level listener so tests can run against an in-memory store without
 * opening a port. `app.listen()` is called only when this module is the
 * process entrypoint (`require.main === module`).
 */
function log(level: string, event: string, data?: Record<string, unknown>): void {
  const entry = { ts: new Date().toISOString(), level, event, ...data };
  process.stderr.write(JSON.stringify(entry) + '\n');
}

interface Metrics {
  register: Registry;
  httpRequestDuration: Histogram<string>;
  nudgesReceived: Counter<string>;
  nudgesAcked: Counter<string>;
  nudgeQueueDepth: Gauge<string>;
  deadLetterCount: Counter<string>;
}

function buildMetrics(): Metrics {
  const register = new Registry();
  collectDefaultMetrics({ register });
  return {
    register,
    httpRequestDuration: new Histogram({
      name: 'messaging_http_request_duration_seconds',
      help: 'HTTP request duration in seconds',
      labelNames: ['method', 'path', 'status'],
      buckets: [0.01, 0.05, 0.1, 0.5, 1, 5],
      registers: [register],
    }),
    nudgesReceived: new Counter({ name: 'messaging_nudges_received_total', help: 'Total nudges received', labelNames: ['from', 'to'], registers: [register] }),
    nudgesAcked: new Counter({ name: 'messaging_nudges_acknowledged_total', help: 'Total nudges acknowledged', registers: [register] }),
    nudgeQueueDepth: new Gauge({ name: 'messaging_nudge_queue_depth', help: 'Number of unacknowledged nudges', registers: [register] }),
    deadLetterCount: new Counter({ name: 'messaging_dead_letter_total', help: 'Total messages dead-lettered', registers: [register] }),
  };
}

function registerRequestLogging(app: Express, metrics: Metrics): void {
  app.use((req, res, next) => {
    const start = process.hrtime.bigint();
    res.on('finish', () => {
      const durationMs = Number(process.hrtime.bigint() - start) / 1e6;
      const route = req.route?.path || req.path;
      metrics.httpRequestDuration.labels(req.method, route, String(res.statusCode)).observe(durationMs / 1000);
      log('info', 'http.request', { method: req.method, path: req.path, status: res.statusCode, durationMs: Math.round(durationMs) });
    });
    next();
  });
}

function registerHealthMetricsRoutes(app: Express, store: MessageStore, metrics: Metrics): void {
  app.get('/metrics', async (_req, res) => {
    metrics.nudgeQueueDepth.set(store.getStats().pending);
    res.set('Content-Type', metrics.register.contentType);
    res.end(await metrics.register.metrics());
  });
  app.get('/health', (_req, res) => {
    res.json({ status: 'ok', port: PORT, ...store.getStats() });
  });
}

function registerNudgeRoutes(app: Express, store: MessageStore, metrics: Metrics, worker?: DeliveryWorker): void {
  app.post('/api/nudge', (req, res) => {
    // #2804 — only the MCP server is the canonical caller. Reject others.
    // Tests and migration callers can opt out via PULSE_ALLOW_DIRECT_POST=1.
    const mcpHeader = req.headers['x-chorus-mcp-caller'];
    const allowDirect = process.env.PULSE_ALLOW_DIRECT_POST === '1';
    if (!mcpHeader && !allowDirect) {
      return res.status(403).json({
        error: 'not-canonical-caller',
        message: 'POST /api/nudge accepts only MCP-server calls. Use the chorus_nudge_message MCP tool from a Claude session.',
      });
    }
    const { from, to, content, traceId: bodyTraceId } = req.body;
    if (!from || !to || !content) return res.status(400).json({ error: 'from, to, content required' });
    // #2765 AC3: prefer X-Chorus-Trace-Id header (canonical UUIDv7); fall back
    // to body.traceId for backward-compat with senders that haven't migrated.
    const headerTrace = req.headers['x-chorus-trace-id'];
    const traceId = (typeof headerTrace === 'string' ? headerTrace : undefined) || bodyTraceId || undefined;
    // #3032: mark teammate nudges so the receiving session can distinguish them
    // from Jeff's own typed prompts (input_classifier already recognizes/strips
    // the "[nudge from" prefix; nothing re-added it after the #2804 refactor).
    // Idempotent — never double-prefix.
    const tsBoston = new Date().toLocaleString('sv-SE', { timeZone: 'America/New_York' }).slice(0, 16);
    const marked = content.startsWith('[nudge from')
      ? content
      : `[nudge from ${from} | ${tsBoston} Boston] ${content}`;
    // #3335 Pattern 7 — drop an identical nudge re-posted within the dedup window.
    // The spine/store is not touched for a dup; the caller gets ok so a retry isn't an error.
    if (seenRecently(dedupeKey(from, to, marked), Date.now(), recentNudges, DEDUP_WINDOW_MS)) {
      log('info', 'nudge.deduped', { from, to, chars: marked.length, trace_id: traceId || undefined });
      return res.json({ ok: true, deduped: true });
    }
    const id = store.sendNudge(from, to, marked, traceId);
    metrics.nudgesReceived.labels(from, to).inc();
    log('info', 'nudge.stored', { id, from, to, chars: marked.length, trace_id: traceId || undefined });
    // #2727 AC2: enqueue for async delivery via worker. No-op if worker not wired (tests).
    if (worker) {
      worker.enqueue({ id, from, to, content: marked, delivery_attempts: 0, trace_id: traceId || null }).catch(() => { /* worker handles its own state */ });
    }
    res.json({ ok: true, id, traceId });
  });
  // #2664: GET /api/nudge/:role/pending retired. Pending count comes from
  // the spine fold (nudge.emitted minus nudge.surfaced) via
  // chorus-hooks/nudge_poll, not from messages.db. The pulse JSON shape
  // is unchanged; assemble_nudges in pulse.rs sources from spine.
  // #2435 wedge 7d retired the ack/attempt HTTP surface alongside this.
}

// #3343 — Jeff's Clearing input. Rides nudge's TRANSPORT (worker, retry,
// per-role serialization) without nudge's CONTRACT:
//   - NO [nudge from] framing — content reaches the session RAW. The framing
//     marks input as relayed-peer-traffic (card_approval_responder ignores it,
//     input_classifier strips it) and would silently strip Jeff's authority.
//   - NO dedup window — Jeff resending the same words after a failure is
//     intentional, never an accidental double-fire.
//   - Spine family jeff.input.* (worker emits via row.kind) — keeps the nudge
//     fold (emitted − surfaced − surface.failed) clean of foreign rows.
// Caller check mirrors #2804: only the Clearing server posts here (marker
// header), with the same test/migration escape hatch.
function registerJeffInputRoutes(app: Express, store: MessageStore, worker?: DeliveryWorker): void {
  app.post('/api/jeff-input', (req, res) => {
    const clearingHeader = req.headers['x-chorus-clearing-caller'];
    const allowDirect = process.env.PULSE_ALLOW_DIRECT_POST === '1';
    if (!clearingHeader && !allowDirect) {
      return res.status(403).json({
        error: 'not-canonical-caller',
        message: 'POST /api/jeff-input accepts only Clearing-server calls (X-Chorus-Clearing-Caller).',
      });
    }
    const { to, content, traceId: bodyTraceId } = req.body;
    if (!to || !content) return res.status(400).json({ error: 'to, content required' });
    const headerTrace = req.headers['x-chorus-trace-id'];
    const traceId = (typeof headerTrace === 'string' ? headerTrace : undefined) || bodyTraceId || undefined;
    const id = store.sendJeffInput(to, content, traceId);
    log('info', 'jeff.input.stored', { id, to, chars: content.length, trace_id: traceId || undefined });
    if (worker) {
      worker.enqueue({ id, from: 'jeff', to, content, delivery_attempts: 0, trace_id: traceId || null, kind: 'jeff-input' }).catch(() => { /* worker handles its own state */ });
    }
    res.json({ ok: true, id, traceId });
  });
}

function registerChatRoutes(app: Express, store: MessageStore): void {
  app.post('/api/chat/start', (req, res) => {
    const { roleA, roleB, topic } = req.body;
    if (!roleA || !roleB) return res.status(400).json({ error: 'roleA, roleB required' });
    res.json({ ok: true, id: store.startChat(roleA, roleB, topic || 'chat') });
  });
  app.post('/api/chat/:id/message', (req, res) => {
    const { from, content } = req.body;
    if (!from || !content) return res.status(400).json({ error: 'from, content required' });
    res.json({ ok: true, id: store.chatMessage(req.params.id, from, content) });
  });
  app.get('/api/chat/:id/messages', (req, res) => {
    const sinceId = req.query.since ? parseInt(req.query.since as string) : undefined;
    res.json(store.getChatMessages(req.params.id, sinceId));
  });
  app.post('/api/chat/:id/end', (req, res) => {
    store.endChat(req.params.id);
    res.json({ ok: true });
  });
}

// #2664: registerDeadLetterRoutes retired. GET /api/dead-letter and
// POST /api/dead-letter/:id/replay had zero production callers; the
// delivery model that produced dead-letter state (recordDeliveryAttempt
// from inject-watcher) was retired by #2435.

function registerStateAndQueryRoutes(app: Express, store: MessageStore): void {
  app.post('/api/board-event', (req, res) => {
    const { from, content } = req.body;
    if (!from || !content) return res.status(400).json({ error: 'from, content required' });
    res.json({ ok: true, id: store.recordBoardEvent(from, content) });
  });
  // #2632: POST/GET /api/role-state retired. Pulse's role-state HTTP API
  // was parallel to chorus-hook-shim CLI with zero callers across the
  // codebase — no-competing-implementations applied. Source of truth:
  // chorus-hook-shim role-state subcommand → /tmp/claude-team-scan/.

  app.get('/api/messages', (req, res) => {
    res.json(store.queryMessages({
      type: req.query.type as string | undefined,
      from: req.query.from as string | undefined,
      to: req.query.to as string | undefined,
      since: req.query.since as string | undefined,
      limit: req.query.limit ? parseInt(req.query.limit as string) : 50,
    }));
  });
  app.get('/api/stats', (_req, res) => {
    res.json(store.getStats());
  });
}

export function createApp(store: MessageStore, worker?: DeliveryWorker): Express {
  const app = express();
  app.use(express.json());
  const metrics = buildMetrics();
  registerRequestLogging(app, metrics);
  registerHealthMetricsRoutes(app, store, metrics);
  registerNudgeRoutes(app, store, metrics, worker);
  registerJeffInputRoutes(app, store, worker);
  registerChatRoutes(app, store);
  registerStateAndQueryRoutes(app, store);
  return app;
}

// #2727 AC2/AC12: production-side worker dependencies. Tests inject mocks
// via DeliveryWorker constructor; this block wires the real chorus-inject
// binary + chorus.log spine writer for the live service.
/* istanbul ignore next */
function buildRuntimeDeps(): { runInject: RunInject; emitSpine: EmitSpine; selfTest: SelfTest } {
  const injectBin = process.env.CHORUS_INJECT_BIN || path.join(os.homedir(), '.chorus', 'bin', 'chorus-inject');
  const chorusLog = process.env.CHORUS_LOG || path.join(os.homedir(), '.chorus', 'chorus.log');

  const runInject: RunInject = (to, content, from) => new Promise(resolve => {
    // #3125: route by tty when the target role has a LIVE registration.
    // planDelivery returns the legacy `[role, content]` name-match args when
    // nothing is registered, so this is inert (= today's behavior) until the
    // SessionStart registry is populated — as-is delivery can never strand.
    // #3352 AC-0 — sender-aware plan: resolve BOTH ends so a delivery whose
    // target collides with the sender's own session defers to the fold
    // instead of keystroking the sender (the 2026-06-11 misdelivery).
    const targetReg = resolveRoleTarget(to);
    const senderReg = from ? resolveRoleTarget(from) : null;
    const plan = planDelivery(targetReg, to, content, senderReg);
    const targetDesc = plan.kind === 'inject'
      ? (plan.args[0] === '--vscode' ? 'vscode-focused' : plan.args[0] === '--tty' ? `tty:${plan.args[1]}` : `name-match:${plan.args[0]}`)
      : `deferred:${plan.reason}`;
    if (plan.kind === 'defer') {
      // VS-Code-hosted target: osascript would leak into the focused app.
      // Hand to the inbox/fold via the worker's deferred path — no keystroke.
      resolve({ rc: 0, stderr: '', deferred: true, deferReason: plan.reason, target: targetDesc });
      return;
    }
    const proc = spawn(injectBin, plan.args, {
      stdio: ['ignore', 'ignore', 'pipe'],
      // #2804 — _NUDGE_PULSE_INTERNAL marks this as the canonical caller;
      // chorus-inject rejects shell-direct calls that lack this env.
      env: { ...process.env, _NUDGE_PULSE_INTERNAL: '1' },
    });
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d.toString(); });
    proc.on('close', rc => resolve({ rc: rc ?? 1, stderr, target: targetDesc }));
    proc.on('error', e => resolve({ rc: 127, stderr: e.message }));
  });

  const emitSpine: EmitSpine = async (event, fields) => {
    // Field name is `timestamp` (not `ts`) to match the canonical chorus.log
    // convention used by the readers (Loki processors, observer.rs, audit
    // tooling). Found during #2764 cutover verification: nudge.surfaced was
    // firing but readers couldn't parse the timestamp because the field was
    // named `ts`. pulse's own messaging.log uses `ts` — that stays.
    const line = JSON.stringify({ timestamp: new Date().toISOString(), event, role: 'pulse', ...fields }) + '\n';
    try { await appendFile(chorusLog, line); } catch { /* best-effort spine write */ }
  };

  // Per Kade gemba 2026-05-07: chorus-inject does not have --self-test today.
  // --count-windows DOES exist (main.rs) and exercises the binary + system
  // events permission without typing into a real terminal. Use it as the
  // smoke probe; rc=0 means binary alive + TCC granted. Pattern is generic
  // ("claude") since pulse boots once for the whole team, not per-role.
  const selfTest: SelfTest = () => new Promise(resolve => {
    const proc = spawn(injectBin, ['--count-windows', 'claude'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      // #2804 — same canonical-caller env as runInject above; smoke probe
      // counts as pulse-internal.
      env: { ...process.env, _NUDGE_PULSE_INTERNAL: '1' },
    });
    let stdout = '';
    let stderr = '';
    proc.stdout.on('data', d => { stdout += d.toString(); });
    proc.stderr.on('data', d => { stderr += d.toString(); });
    proc.on('close', rc => resolve({ rc: rc ?? 1, stderr: stderr || stdout }));
    proc.on('error', e => resolve({ rc: 127, stderr: e.message }));
  });

  return { runInject, emitSpine, selfTest };
}

// Run as a server only when this file is the process entrypoint. Tests import
// `createApp` and do not call `app.listen()`.
/* istanbul ignore next */
if (require.main === module) {
  const store = new MessageStore();
  const { runInject, emitSpine, selfTest } = buildRuntimeDeps();
  const worker = new DeliveryWorker(store, runInject, emitSpine, undefined, undefined, selfTest);

  process.on('SIGTERM', () => { store.close(); process.exit(0); });
  process.on('SIGINT', () => { store.close(); process.exit(0); });

  // #2727 boot sequence (Silas review 2026-05-07):
  // 1. startupSmoke — fail-fast, exit 1 if TCC cold-start probe fails
  // 2. scanAndRequeue — OPT-IN via PULSE_REQUEUE_ON_BOOT=1. Default off
  //    avoids stampede-on-boot (10k+ row burst at deploy moment), operator
  //    inspect/drain pathway preserved, deploy-time spine stays quiet.
  //    When enabled, requeue is the explicit deliberate operator action.
  // 3. app.listen — only after smoke + (optional) scan complete
  (async () => {
    try {
      await worker.startupSmoke();
    } catch (e) {
      process.stderr.write(JSON.stringify({ event: 'startup.smoke.failed', error: String(e) }) + '\n');
      process.exit(1);
    }

    if (process.env.PULSE_REQUEUE_ON_BOOT === '1') {
      try {
        const pendingCount = store.getPendingDeliveries().length;
        process.stderr.write(JSON.stringify({ event: 'startup.requeue.start', pending: pendingCount }) + '\n');
        await worker.scanAndRequeue();
        process.stderr.write(JSON.stringify({ event: 'startup.requeue.complete', enqueued: pendingCount }) + '\n');
      } catch (e) {
        process.stderr.write(JSON.stringify({ event: 'startup.requeue.failed', error: String(e) }) + '\n');
        process.exit(1);
      }
    } else {
      process.stderr.write(JSON.stringify({ event: 'startup.requeue.skipped', reason: 'PULSE_REQUEUE_ON_BOOT unset' }) + '\n');
    }

    const app = createApp(store, worker);
    app.listen(PORT, () => {
      process.stderr.write(JSON.stringify({ event: 'startup', port: PORT, ...store.getStats() }) + '\n');
    });
  })();
}
