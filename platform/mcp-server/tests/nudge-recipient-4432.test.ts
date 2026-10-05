// @test-type: integration — a local HTTP stand-in for the roles door and pulse; no live services
// @domain: messages
/**
 * #4432 — chorus_nudge_message accepts any role the roles door lists (Abby
 * Normal included) and refuses a name it does not list, naming the door's
 * peers. The door and pulse are a local server; nothing reaches :3360 or :3475.
 */
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { createServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';
import { resetRoleSetsCache } from '../src/peers';

const DOOR = JSON.stringify({ data: [
  { name: 'abby-normal', roleKind: 'agent' }, { name: 'jeff', roleKind: 'human' },
  { name: 'kade', roleKind: 'agent' }, { name: 'silas', roleKind: 'agent' }, { name: 'wren', roleKind: 'agent' },
] });

async function standIn(): Promise<{ base: string; posted: string[]; close: () => void }> {
  const posted: string[] = [];
  const srv: Server = createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      if (req.url?.startsWith('/v1/roles/roles')) { res.writeHead(200, { 'content-type': 'application/json' }); res.end(DOOR); return; }
      if (req.url?.startsWith('/api/nudge')) { posted.push(body); res.writeHead(200, { 'content-type': 'application/json' }); res.end('{"ok":true,"id":1}'); return; }
      res.writeHead(404); res.end();
    });
  });
  await new Promise<void>((r) => srv.listen(0, '127.0.0.1', () => r()));
  const port = (srv.address() as { port: number }).port;
  return { base: `http://127.0.0.1:${port}`, posted, close: () => srv.close() };
}

async function client(): Promise<Client> {
  const server = buildMcpServer(() => 'silas');
  const [c, s] = InMemoryTransport.createLinkedPair();
  await server.connect(s);
  const cl = new Client({ name: 'nudge-4432', version: '0.1.0' }, { capabilities: {} });
  await cl.connect(c);
  return cl;
}

test('a nudge to abby-normal is accepted and posted', async () => {
  const s = await standIn();
  const keep = { door: process.env.ATHENA_MAKE_URL, pulse: process.env.CHORUS_PULSE_URL, log: process.env.CHORUS_LOG_FILE };
  process.env.ATHENA_MAKE_URL = s.base; process.env.CHORUS_PULSE_URL = `${s.base}/api/nudge`;
  process.env.CHORUS_LOG_FILE = join(tmpdir(), `nudge-4432-${process.pid}.log`);
  resetRoleSetsCache();
  try {
    const r = await (await client()).callTool({ name: 'chorus_nudge_message', arguments: { to: 'abby-normal', message: 'hello abby' } });
    assert.notEqual(r.isError, true, JSON.stringify(r.content));
    assert.equal(s.posted.length, 1);
    assert.match(s.posted[0], /"to":"abby-normal"/);
  } finally {
    s.close(); process.env.ATHENA_MAKE_URL = keep.door; process.env.CHORUS_PULSE_URL = keep.pulse; process.env.CHORUS_LOG_FILE = keep.log;
  }
});

test('NEGATIVE PROOF: a name the door does not list is refused and never posted', async () => {
  const s = await standIn();
  const keep = { door: process.env.ATHENA_MAKE_URL, pulse: process.env.CHORUS_PULSE_URL, log: process.env.CHORUS_LOG_FILE };
  process.env.ATHENA_MAKE_URL = s.base; process.env.CHORUS_PULSE_URL = `${s.base}/api/nudge`;
  process.env.CHORUS_LOG_FILE = join(tmpdir(), `nudge-4432-${process.pid}.log`);
  resetRoleSetsCache();
  try {
    let msg = '';
    try {
      const r = await (await client()).callTool({ name: 'chorus_nudge_message', arguments: { to: 'nobody', message: 'hi' } });
      msg = JSON.stringify(r.content);
    } catch (e) { msg = (e as Error).message; }
    assert.match(msg, /Unknown recipient 'nobody'/);
    assert.match(msg, /abby-normal \| jeff \| kade \| silas \| wren/);
    assert.equal(s.posted.length, 0);
  } finally {
    s.close(); process.env.ATHENA_MAKE_URL = keep.door; process.env.CHORUS_PULSE_URL = keep.pulse; process.env.CHORUS_LOG_FILE = keep.log;
  }
});
