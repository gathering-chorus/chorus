/**
 * Messaging Tier Service — REST API for persistent messaging (#1755)
 *
 * Replaces /tmp file queues. SQLite-backed, queryable, observable.
 * CLI wrappers (nudge, chat.sh) call this API.
 */

import express, { Express } from 'express';
import { MessageStore } from './store';
import { DeliveryWorker, type RunInject, type EmitSpine, type SelfTest } from './delivery-worker';
import { Registry, Counter, Histogram, Gauge, collectDefaultMetrics } from 'prom-client';
import { spawn } from 'child_process';
import { appendFile } from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

const PORT = parseInt(process.env.MESSAGING_PORT || '3475');

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
    const { from, to, content, traceId } = req.body;
    if (!from || !to || !content) return res.status(400).json({ error: 'from, to, content required' });
    const id = store.sendNudge(from, to, content);
    metrics.nudgesReceived.labels(from, to).inc();
    log('info', 'nudge.stored', { id, from, to, chars: content.length, traceId: traceId || undefined });
    // #2727 AC2: enqueue for async delivery via worker. No-op if worker not wired (tests).
    if (worker) {
      worker.enqueue({ id, from, to, content, delivery_attempts: 0 }).catch(() => { /* worker handles its own state */ });
    }
    res.json({ ok: true, id, traceId });
  });
  // #2664: GET /api/nudge/:role/pending retired. Pending count comes from
  // the spine fold (nudge.emitted minus nudge.surfaced) via
  // chorus-hooks/nudge_poll, not from messages.db. The pulse JSON shape
  // is unchanged; assemble_nudges in pulse.rs sources from spine.
  // #2435 wedge 7d retired the ack/attempt HTTP surface alongside this.
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

  const runInject: RunInject = (to, content) => new Promise(resolve => {
    const proc = spawn(injectBin, [to, content], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d.toString(); });
    proc.on('close', rc => resolve({ rc: rc ?? 1, stderr }));
    proc.on('error', e => resolve({ rc: 127, stderr: e.message }));
  });

  const emitSpine: EmitSpine = async (event, fields) => {
    const line = JSON.stringify({ ts: new Date().toISOString(), event, role: 'pulse', ...fields }) + '\n';
    try { await appendFile(chorusLog, line); } catch { /* best-effort spine write */ }
  };

  const selfTest: SelfTest = () => new Promise(resolve => {
    const proc = spawn(injectBin, ['--self-test'], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    proc.stderr.on('data', d => { stderr += d.toString(); });
    proc.on('close', rc => resolve({ rc: rc ?? 1, stderr }));
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

  (async () => {
    try {
      await worker.startupSmoke();
    } catch (e) {
      process.stderr.write(JSON.stringify({ event: 'startup.smoke.failed', error: String(e) }) + '\n');
      process.exit(1);
    }
    await worker.scanAndRequeue();

    const app = createApp(store, worker);
    app.listen(PORT, () => {
      process.stderr.write(JSON.stringify({ event: 'startup', port: PORT, ...store.getStats() }) + '\n');
    });
  })();
}
