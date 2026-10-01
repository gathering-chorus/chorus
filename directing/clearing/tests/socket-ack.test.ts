// @test-type: unit — a socket.io client talks to the in-process Clearing; pulse is a stub on an ephemeral port; brings its own world.
// @card: #1840
// @owner: wren
/**
 * Socket.IO ack — #1934, contract rewritten #3646.
 *
 * Jeff presses send: the server must ack ({ok:true}) once his message is
 * accepted, and each role's hand-off result arrives as a 'delivery-status'
 * event the page renders. #4417 — this file used to match regexes over
 * server.ts and index.html; it now sends a real 'jeff-message' over a socket.
 * The page's sending/sent/failed states belong to the ui lane.
 */
import * as fs from 'fs';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import type { AddressInfo } from 'net';
import { io as ioClient, Socket } from 'socket.io-client';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'socket-ack-'));
let pulseAnswer = 200;
const pulseSeen: string[] = [];
const pulse = http.createServer((req, res) => {
  let raw = '';
  req.on('data', (c) => { raw += c; });
  req.on('end', () => { pulseSeen.push(raw); res.statusCode = pulseAnswer; res.end(pulseAnswer === 200 ? '{"ok":true}' : '{"error":"stub refused"}'); });
});

let srv: { server: http.Server; io: { close: () => void } };
let base = '';
const clients: Socket[] = [];

beforeAll(async () => {
  await new Promise<void>((r) => pulse.listen(0, '127.0.0.1', () => r()));
  process.env.PULSE_URL = `http://127.0.0.1:${(pulse.address() as AddressInfo).port}`;
  process.env.CHORUS_ROOT = TMP;
  process.env.CLEARING_SCAN_DIR = TMP;
  process.env.CLEARING_PROJECTS_DIR = TMP;
  process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
  srv = require('../src/server');
  await new Promise<void>((r) => srv.server.listen(0, '127.0.0.1', () => r()));
  base = `http://127.0.0.1:${(srv.server.address() as AddressInfo).port}`;
});

afterAll(async () => {
  for (const c of clients) c.close();
  srv?.io.close();
  if (srv) await new Promise<void>((r) => srv.server.close(() => r()));
  await new Promise<void>((r) => pulse.close(() => r()));
  fs.rmSync(TMP, { recursive: true, force: true });
});

beforeEach(() => { pulseAnswer = 200; pulseSeen.length = 0; });

async function connect(): Promise<Socket> {
  const c = ioClient(base, { forceNew: true, transports: ['websocket'] });
  clients.push(c);
  await new Promise<void>((resolve, reject) => { c.on('connect', () => resolve()); c.on('connect_error', reject); });
  return c;
}

function send(c: Socket, text: string): Promise<{ ack: { ok: boolean; error?: string }; status: { target: string; ok: boolean; error?: string } }> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('no ack and delivery-status within 5s')), 5000);
    let ack: { ok: boolean; error?: string } | null = null;
    c.once('delivery-status', (status) => {
      const finish = () => { clearTimeout(timer); resolve({ ack: ack!, status }); };
      if (ack) finish(); else setTimeout(finish, 50);
    });
    c.emit('jeff-message', { text }, (result: { ok: boolean; error?: string }) => { ack = result; });
  });
}

describe('#1934 / #3646 Jeff sends over the socket', () => {
  test('the server acks ok and reports the hand-off to the role as delivered', async () => {
    const c = await connect();
    const r = await send(c, '@wren hello from the socket test');
    expect(r.ack).toEqual({ ok: true });
    expect(r.status).toEqual({ target: 'wren', ok: true });
    expect(pulseSeen).toHaveLength(1);
  });

  test('pulse refuses → the message is still accepted, and the role\'s status says failed', async () => {
    pulseAnswer = 500;
    const c = await connect();
    const r = await send(c, '@kade this one will not land');
    expect(r.ack).toEqual({ ok: true });
    expect(r.status.target).toBe('kade');
    expect(r.status.ok).toBe(false);
    expect(r.status.error).toMatch(/^pulse 500/);
  });

  test('NEGATIVE PROOF: an empty message is refused in the ack, and nothing reaches pulse', async () => {
    const c = await connect();
    const ack = await new Promise<{ ok: boolean; error?: string }>((resolve) => c.emit('jeff-message', { text: '   ' }, resolve));
    expect(ack).toEqual({ ok: false, error: 'empty' });
    await new Promise((r) => setTimeout(r, 100));
    expect(pulseSeen).toHaveLength(0);
  });
});
