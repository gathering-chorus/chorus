/**
 * Messaging Tier Service — REST API for persistent messaging (#1755)
 *
 * Replaces /tmp file queues. SQLite-backed, queryable, observable.
 * CLI wrappers (nudge.sh, chat.sh) call this API.
 */

import express from 'express';
import { MessageStore } from './store';

const PORT = parseInt(process.env.MESSAGING_PORT || '3475');
const app = express();
app.use(express.json());

const store = new MessageStore();

// --- Health ---
app.get('/health', (_req, res) => {
  const stats = store.getStats();
  res.json({ status: 'ok', port: PORT, ...stats });
});

// --- Nudges ---
app.post('/api/nudge', (req, res) => {
  const { from, to, content } = req.body;
  if (!from || !to || !content) return res.status(400).json({ error: 'from, to, content required' });
  const id = store.sendNudge(from, to, content);
  res.json({ ok: true, id });
});

app.get('/api/nudge/:role/pending', (req, res) => {
  const nudges = store.getPendingNudges(req.params.role);
  res.json(nudges);
});

app.post('/api/nudge/:id/ack', (req, res) => {
  store.acknowledgeNudge(parseInt(req.params.id));
  res.json({ ok: true });
});

app.post('/api/nudge/:role/ack-all', (req, res) => {
  const count = store.acknowledgeAllNudges(req.params.role);
  res.json({ ok: true, acknowledged: count });
});

// --- Chats ---
app.post('/api/chat/start', (req, res) => {
  const { roleA, roleB, topic } = req.body;
  if (!roleA || !roleB) return res.status(400).json({ error: 'roleA, roleB required' });
  const id = store.startChat(roleA, roleB, topic || 'chat');
  res.json({ ok: true, id });
});

app.post('/api/chat/:id/message', (req, res) => {
  const { from, content } = req.body;
  if (!from || !content) return res.status(400).json({ error: 'from, content required' });
  const msgId = store.chatMessage(req.params.id, from, content);
  res.json({ ok: true, id: msgId });
});

app.get('/api/chat/:id/messages', (req, res) => {
  const sinceId = req.query.since ? parseInt(req.query.since as string) : undefined;
  const messages = store.getChatMessages(req.params.id, sinceId);
  res.json(messages);
});

app.post('/api/chat/:id/end', (req, res) => {
  store.endChat(req.params.id);
  res.json({ ok: true });
});

// --- Dead Letter ---
app.get('/api/dead-letter', (_req, res) => {
  const messages = store.getDeadLetters({ limit: 50 });
  res.json(messages);
});

app.post('/api/dead-letter/:id/replay', (req, res) => {
  store.replayDeadLetter(parseInt(req.params.id));
  res.json({ ok: true });
});

app.post('/api/nudge/:id/attempt', (req, res) => {
  const result = store.recordDeliveryAttempt(parseInt(req.params.id));
  res.json({ ok: true, ...result });
});

// --- Board Events ---
app.post('/api/board-event', (req, res) => {
  const { from, content } = req.body;
  if (!from || !content) return res.status(400).json({ error: 'from, content required' });
  const id = store.recordBoardEvent(from, content);
  res.json({ ok: true, id });
});

// --- Role State ---
app.post('/api/role-state', (req, res) => {
  const { role, state, card, detail } = req.body;
  if (!role || !state) return res.status(400).json({ error: 'role, state required' });
  store.setRoleState(role, state, card, detail);
  res.json({ ok: true });
});

app.get('/api/role-state/:role', (req, res) => {
  const state = store.getRoleState(req.params.role);
  res.json(state || { error: 'not found' });
});

// --- Query ---
app.get('/api/messages', (req, res) => {
  const opts = {
    type: req.query.type as string | undefined,
    from: req.query.from as string | undefined,
    to: req.query.to as string | undefined,
    since: req.query.since as string | undefined,
    limit: req.query.limit ? parseInt(req.query.limit as string) : 50,
  };
  const messages = store.queryMessages(opts);
  res.json(messages);
});

// --- Stats ---
app.get('/api/stats', (_req, res) => {
  res.json(store.getStats());
});

// Graceful shutdown
process.on('SIGTERM', () => { store.close(); process.exit(0); });
process.on('SIGINT', () => { store.close(); process.exit(0); });

app.listen(PORT, () => {
  console.log(`Messaging service listening on http://localhost:${PORT}`);
});
