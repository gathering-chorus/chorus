/**
 * Messaging Tier Service — REST API for persistent messaging (#1755)
 *
 * Replaces /tmp file queues. SQLite-backed, queryable, observable.
 * CLI wrappers (nudge, chat.sh) call this API.
 */

import express, { Express } from 'express';
import { MessageStore } from './store';
import { Registry, Counter, Histogram, Gauge, collectDefaultMetrics } from 'prom-client';

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

function registerNudgeRoutes(app: Express, store: MessageStore, metrics: Metrics): void {
  app.post('/api/nudge', (req, res) => {
    const { from, to, content, traceId } = req.body;
    if (!from || !to || !content) return res.status(400).json({ error: 'from, to, content required' });
    const id = store.sendNudge(from, to, content);
    metrics.nudgesReceived.labels(from, to).inc();
    log('info', 'nudge.stored', { id, from, to, chars: content.length, traceId: traceId || undefined });
    res.json({ ok: true, id, traceId });
  });
  app.get('/api/nudge/:role/pending', (req, res) => {
    res.json(store.getPendingNudges(req.params.role));
  });
  app.post('/api/nudge/:id/ack', (req, res) => {
    store.acknowledgeNudge(parseInt(req.params.id));
    metrics.nudgesAcked.inc();
    log('info', 'nudge.acknowledged', { id: req.params.id });
    res.json({ ok: true });
  });
  app.post('/api/nudge/:role/ack-all', (req, res) => {
    const count = store.acknowledgeAllNudges(req.params.role);
    metrics.nudgesAcked.inc(count);
    log('info', 'nudge.ack-all', { role: req.params.role, count });
    res.json({ ok: true, acknowledged: count });
  });
  app.post('/api/nudge/:id/attempt', (req, res) => {
    const id = parseInt(req.params.id);
    const result = store.recordDeliveryAttempt(id);
    if (result.deadLettered) {
      metrics.deadLetterCount.inc();
      log('warn', 'nudge.dead-lettered', { id, reason: 'max delivery attempts exceeded' });
    } else {
      log('info', 'nudge.delivery-attempt', { id });
    }
    res.json({ ok: true, ...result });
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

function registerDeadLetterRoutes(app: Express, store: MessageStore): void {
  app.get('/api/dead-letter', (_req, res) => {
    res.json(store.getDeadLetters({ limit: 50 }));
  });
  app.post('/api/dead-letter/:id/replay', (req, res) => {
    const id = parseInt(req.params.id);
    store.replayDeadLetter(id);
    log('info', 'dead-letter.replayed', { id });
    res.json({ ok: true });
  });
}

function registerStateAndQueryRoutes(app: Express, store: MessageStore): void {
  app.post('/api/board-event', (req, res) => {
    const { from, content } = req.body;
    if (!from || !content) return res.status(400).json({ error: 'from, content required' });
    res.json({ ok: true, id: store.recordBoardEvent(from, content) });
  });
  app.post('/api/role-state', (req, res) => {
    const { role, state, card, detail } = req.body;
    if (!role || !state) return res.status(400).json({ error: 'role, state required' });
    store.setRoleState(role, state, card, detail);
    res.json({ ok: true });
  });
  app.get('/api/role-state/:role', (req, res) => {
    res.json(store.getRoleState(req.params.role) || { error: 'not found' });
  });
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

export function createApp(store: MessageStore): Express {
  const app = express();
  app.use(express.json());
  const metrics = buildMetrics();
  registerRequestLogging(app, metrics);
  registerHealthMetricsRoutes(app, store, metrics);
  registerNudgeRoutes(app, store, metrics);
  registerChatRoutes(app, store);
  registerDeadLetterRoutes(app, store);
  registerStateAndQueryRoutes(app, store);
  return app;
}

// Run as a server only when this file is the process entrypoint. Tests import
// `createApp` and do not call `app.listen()`.
/* istanbul ignore next */
if (require.main === module) {
  const store = new MessageStore();
  const app = createApp(store);
  process.on('SIGTERM', () => { store.close(); process.exit(0); });
  process.on('SIGINT', () => { store.close(); process.exit(0); });
  app.listen(PORT, () => {
    process.stderr.write(JSON.stringify({ event: 'startup', port: PORT, ...store.getStats() }) + '\n');
  });
}
