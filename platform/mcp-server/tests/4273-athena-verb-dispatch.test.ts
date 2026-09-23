// @test-type: unit — in-memory MCP transport + stub binaries on CHORUS_BIN
//
// #4273 — the four athena tools shared one dispatch arm whose body hardcoded
// executeAthenaValidate, so calling athena-model / athena-make / athena-deploy
// over MCP ran athena-VALIDATE and returned its sweep. Measured 2026-09-22:
// `athena-model delete --kind principle --name …` came back
// {"verb":"athena-validate","verdict":"dirty","issues":45132} and the row it
// was asked to delete was still there. A tool that runs a different verb than
// its name and reports success is worse than one that refuses.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

// Each stub prints its OWN name, so the answer names the binary that ran.
function stubAll(dir: string) {
  for (const v of ['athena-model', 'athena-make', 'athena-deploy', 'athena-validate']) {
    const bin = path.join(dir, v);
    fs.writeFileSync(bin, `#!/bin/bash\necho "ran=${v} args=$*"\n`);
    fs.chmodSync(bin, 0o755);
  }
}

async function withServer(fn: (client: Client) => Promise<void>) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-dispatch-'));
  stubAll(dir);
  const prev = process.env.CHORUS_BIN; process.env.CHORUS_BIN = dir;
  const server = buildMcpServer(() => 'kade', { runsDir: dir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-dispatch-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally {
    await client.close(); await server.close();
    if (prev === undefined) delete process.env.CHORUS_BIN; else process.env.CHORUS_BIN = prev;
  }
}

async function callVerb(client: Client, name: string, args: string[]) {
  const res = await client.callTool({ name, arguments: { role: 'kade', args } });
  return (res.content as Array<{ text: string }>)[0].text;
}

for (const verb of ['athena-model', 'athena-make', 'athena-deploy', 'athena-validate']) {
  test(`${verb} runs the ${verb} binary, not some other athena verb`, async () => {
    await withServer(async (client) => {
      const text = await callVerb(client, verb, ['delete', '--kind', 'principle']);
      assert.ok(text.includes(`ran=${verb}`), `${verb} dispatched elsewhere: ${text.slice(0, 200)}`);
    });
  });
}

// NEGATIVE PROOF (#3734): the assertion above must be able to fail. Prove the
// stub really does distinguish the verbs — if every stub printed the same
// thing, all four tests would pass no matter which binary ran.
test('NEGATIVE PROOF — the stubs name different binaries, so a wrong dispatch is visible', async () => {
  await withServer(async (client) => {
    const model = await callVerb(client, 'athena-model', []);
    const validate = await callVerb(client, 'athena-validate', []);
    assert.notEqual(model, validate, 'the two verbs returned identical output — the check cannot see a wrong dispatch');
    assert.ok(!model.includes('ran=athena-validate'), 'athena-model ran athena-validate — the #4273 bug');
  });
});
