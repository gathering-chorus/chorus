#!/usr/bin/env node
/**
 * soak.mjs — #3646: the Clearing input soak test.
 *
 * Sends N messages at a fixed interval over one Socket.IO connection and holds
 * the session open between sends (the works-once bug lived in session lifetime,
 * not first-send). Reports per-send: ack latency, ack verdict, delivery-status
 * events received. Exit 0 only when every send acked ok.
 *
 * Run from directing/clearing (resolves socket.io-client from this package):
 *   node scripts/soak.mjs                                  # local, 10 sends, 5s apart
 *   node scripts/soak.mjs --url https://clearing.lightlifeurbangardens.com \
 *     --token-file ~/.chorus/bridge-auth-token --n 10 --interval-ms 120000
 *
 * The AC's full soak = 10 sends over 20 min from each origin (Library, wifi,
 * public) — same tool, three runs, interval 120000.
 */
import { io } from 'socket.io-client';
import { readFileSync } from 'fs';

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > -1 ? process.argv[i + 1] : dflt;
};

const URL = arg('url', 'http://localhost:3470');
const N = parseInt(arg('n', '10'), 10);
const INTERVAL_MS = parseInt(arg('interval-ms', '5000'), 10);
const TOKEN_FILE = arg('token-file', '');
const token = TOKEN_FILE ? readFileSync(TOKEN_FILE.replace('~', process.env.HOME), 'utf-8').trim() : '';

const runId = `soak-${process.pid}`;
const results = [];
let deliveryStatuses = 0;

const socket = io(URL, {
  auth: token ? { token } : {},
  reconnection: true,
  timeout: 30000,
});

socket.on('delivery-status', (s) => {
  deliveryStatuses += 1;
  if (s && s.ok === false) console.log(`  delivery-status: ${s.target} FAILED (${s.error})`);
});

socket.on('connect_error', (e) => console.log(`connect_error: ${e.message}`));
socket.on('disconnect', (r) => console.log(`disconnect: ${r}`));
socket.on('connect', () => console.log(`connected to ${URL} (id ${socket.id})`));

function sendOne(i) {
  return new Promise((resolve) => {
    const started = Date.now();
    const text = `[${runId}] send ${i + 1}/${N} @ ${new Date().toISOString()}`;
    const timer = setTimeout(() => resolve({ i, ok: false, error: 'ack-timeout-10s', ms: 10000 }), 10000);
    if (!socket.connected) {
      clearTimeout(timer);
      return resolve({ i, ok: false, error: 'socket-disconnected-at-send', ms: 0 });
    }
    socket.emit('jeff-message', { text, from: 'soak' }, (ackResult) => {
      clearTimeout(timer);
      resolve({ i, ok: !!(ackResult && ackResult.ok), error: ackResult && ackResult.error, ms: Date.now() - started });
    });
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

socket.on('connect', async function once() {
  socket.off('connect', once);
  for (let i = 0; i < N; i += 1) {
    const r = await sendOne(i);
    results.push(r);
    console.log(`send ${i + 1}/${N}: ${r.ok ? 'ACK' : 'FAIL'} in ${r.ms}ms${r.error ? ` (${r.error})` : ''}`);
    if (i < N - 1) await sleep(INTERVAL_MS);
  }
  const ok = results.filter((r) => r.ok).length;
  console.log(`\nRESULT: ${ok}/${N} acked ok · ${deliveryStatuses} delivery-status events · session held ${Math.round((N - 1) * INTERVAL_MS / 1000)}s`);
  socket.close();
  process.exit(ok === N ? 0 : 1);
});
