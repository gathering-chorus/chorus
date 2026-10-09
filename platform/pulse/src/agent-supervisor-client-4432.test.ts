// @test-type: unit — a local HTTP server on a temp Unix socket stands in for chorus-agentd; no live services
// @domain: messages
// @card: #4432
// @owner: wren
/** #4432 — Pulse's client for an agent's supervisor, against a stand-in socket. */
import { createServer, type Server } from 'node:http';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { LocalAgentSupervisor } from './agent-supervisor';

let dir: string;
let server: Server;
let reply: { status: number; body: string } = { status: 200, body: '{}' };
let last: { method?: string; url?: string; body: string } = { body: '' };

beforeAll(async () => {
  dir = mkdtempSync(join(tmpdir(), 'agentd-4432-'));
  server = createServer((req, res) => {
    let body = '';
    req.on('data', (c: Buffer) => { body += c.toString(); });
    req.on('end', () => { last = { method: req.method, url: req.url, body }; res.writeHead(reply.status); res.end(reply.body); });
  });
  await new Promise<void>((r) => server.listen(join(dir, 'agent.sock'), () => r()));
});
afterAll(async () => { await new Promise<void>((r) => server.close(() => r())); rmSync(dir, { recursive: true, force: true }); });

const client = () => new LocalAgentSupervisor(join(dir, 'agent.sock'));

test('send posts the message to the run and returns its receipt', async () => {
  reply = { status: 200, body: '{"status":"context_delivered"}' };
  expect(await client().send('abby-run-1', 'msg-7', 'hello abby', 'peer_message')).toBe('context_delivered');
  expect(last.method).toBe('POST');
  expect(last.url).toBe('/v1/sessions/abby-run-1/send');
  expect(JSON.parse(last.body)).toEqual({ version: 1, message_id: 'msg-7', input: 'hello abby', kind: 'peer_message' });
});

test('receipts reads the run receipts', async () => {
  reply = { status: 200, body: '{"receipts":{"msg-7":"context_delivered"}}' };
  expect(await client().receipts('abby-run-1')).toEqual({ 'msg-7': 'context_delivered' });
  expect(last.url).toBe('/v1/sessions/abby-run-1/receipts');
});

test('NEGATIVE PROOF: a receipt the contract does not name is an error, not a delivery', async () => {
  reply = { status: 200, body: '{"status":"delivered-probably"}' };
  await expect(client().send('r', 'm', 'x', 'peer_message')).rejects.toThrow('invalid-agent-receipt');
});

test('NEGATIVE PROOF: a refused or unreadable answer is an error', async () => {
  reply = { status: 403, body: '{}' };
  await expect(client().send('r', 'm', 'x', 'human_input')).rejects.toThrow('supervisor-refused');
  reply = { status: 200, body: 'not json' };
  await expect(client().receipts('r')).rejects.toThrow('supervisor-response-invalid');
  reply = { status: 200, body: '{"receipts":null}' };
  await expect(client().receipts('r')).rejects.toThrow('invalid-agent-receipts');
});

test('NEGATIVE PROOF: no supervisor on the socket is an error', async () => {
  await expect(new LocalAgentSupervisor(join(dir, 'nobody.sock')).receipts('r')).rejects.toThrow();
});
