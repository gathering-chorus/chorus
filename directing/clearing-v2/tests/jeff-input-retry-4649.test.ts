// @test-type: unit — a stub fetch stands in for pulse; no live services
// @domain: messages
// @card: #4649
// @owner: wren
// #4649 — three of Jeff's messages were lost on 10-19 (17:32 kade, 17:53 wren,
// 19:50 silas): the hand-off died with a bare "fetch failed" before it reached
// pulse, and nothing retried. A connection that fails before pulse answers is
// retried once; anything pulse answered, or a timeout, is not (jeff-input has
// no dedupe, so a second POST pulse already took would deliver twice).
import { deliverJeffInput, FetchLike } from '../src/jeff-input';

const ok = { ok: true, status: 202, text: async () => '' };
const socketDrop = (code: string) => Object.assign(new TypeError('fetch failed'), { cause: Object.assign(new Error('other side closed'), { code }) });

test('a connection dropped before pulse answers is retried once and delivers', async () => {
  let calls = 0;
  const f: FetchLike = async () => { calls += 1; if (calls === 1) throw socketDrop('UND_ERR_SOCKET'); return ok; };
  expect(await deliverJeffInput('http://pulse', 'silas', 'what do i need to do to finish 4627', f)).toEqual({ ok: true });
  expect(calls).toBe(2);
});

test('two drops in a row fail with the real cause named, not a bare "fetch failed"', async () => {
  let calls = 0;
  const f: FetchLike = async () => { calls += 1; throw socketDrop('ECONNRESET'); };
  const out = await deliverJeffInput('http://pulse', 'kade', 'x', f);
  expect(calls).toBe(2);
  expect(out).toEqual({ ok: false, reason: expect.stringContaining('ECONNRESET') });
});

test('negative proof: an answer from pulse is never retried', async () => {
  let calls = 0;
  const f: FetchLike = async () => { calls += 1; return { ok: false, status: 500, text: async () => 'boom' }; };
  const out = await deliverJeffInput('http://pulse', 'kade', 'x', f);
  expect(calls).toBe(1);
  expect(out).toEqual({ ok: false, reason: expect.stringContaining('pulse 500') });
});

test('negative proof: a timeout is not retried, since pulse may already have the message', async () => {
  let calls = 0;
  const f: FetchLike = async () => { calls += 1; throw Object.assign(new Error('This operation was aborted'), { name: 'AbortError' }); };
  const out = await deliverJeffInput('http://pulse', 'kade', 'x', f);
  expect(calls).toBe(1);
  expect(out.ok).toBe(false);
});

// Silas's ask: a real server that drops the first socket before answering, through
// real fetch, so the cause codes are Node's own and not ones this test invented.
import http from 'node:http';
import type { AddressInfo } from 'node:net';

test('real socket: pulse drops the first connection before answering; one retry delivers once', async () => {
  let seen = 0;
  const bodies: string[] = [];
  const server = http.createServer((req, res) => {
    seen += 1;
    if (seen === 1) { req.socket.destroy(); return; }
    let b = ''; req.on('data', (c) => { b += c; }); req.on('end', () => { bodies.push(b); res.writeHead(202).end(); });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  try {
    expect(await deliverJeffInput(url, 'wren', 'hello')).toEqual({ ok: true });
    expect(bodies).toEqual([JSON.stringify({ to: 'wren', content: 'hello' })]);
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});

test('real socket, negative proof: a 500 from pulse is not retried', async () => {
  let seen = 0;
  const server = http.createServer((_req, res) => { seen += 1; res.writeHead(500).end('boom'); });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  try {
    const out = await deliverJeffInput(url, 'wren', 'hello');
    expect(out).toEqual({ ok: false, reason: expect.stringContaining('pulse 500') });
    expect(seen).toBe(1);
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});
