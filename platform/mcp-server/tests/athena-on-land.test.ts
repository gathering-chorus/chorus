// @test-type: unit — signal is fixture-data: in-memory MCP transport, stub werk-merge + athena-deploy on CHORUS_BIN, stub act on CHORUS_ACT_BIN, tmp runsDir
// #4177 — the land EVENT triggers the model pipeline. werk.yml names no athena step;
// chorus-mcp's werk-merge case scopes the landed sha and starts athena.yml detached
// when it carried model or seed sources. Jeff, 2026-09-17: "what is athena-land why
// is that part of this flow!"
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { execFileSync } from 'node:child_process';
import { buildMcpServer } from '../src/server';

// #4338 — CHORUS_HOME is a real git repo: the trigger runs the LANDED commit's
// athena.yml (git show <sha>:...), and the tree's copy is deliberately different,
// the way canonical still holds the previous land's file at merge time.
const LANDED_MARK = 'landed-commit-workflow';
const TREE_MARK = 'stale-tree-workflow';
let SHA = '';
function gitHome(dir: string): string {
  const home = path.join(dir, 'home'); fs.mkdirSync(path.join(home, '.github', 'workflows'), { recursive: true });
  const git = (...a: string[]) => execFileSync('git', ['-C', home, ...a], { encoding: 'utf8' }).trim();
  git('init', '-q'); git('config', 'user.email', 't@t'); git('config', 'user.name', 't');
  const wf = path.join(home, '.github', 'workflows', 'athena.yml');
  fs.writeFileSync(wf, `name: athena\n# ${LANDED_MARK}\njobs: {}\n`);
  git('add', '.'); git('commit', '-qm', 'landed');
  SHA = git('rev-parse', 'HEAD');
  fs.writeFileSync(wf, `name: athena\n# ${TREE_MARK}\njobs: {}\n`);
  return home;
}

function stub(dir: string, name: string, body: string): string {
  const bin = path.join(dir, name);
  fs.writeFileSync(bin, `#!/bin/bash\n${body}\n`);
  fs.chmodSync(bin, 0o755);
  return bin;
}

async function withServer(scopeOut: string | null, fn: (client: Client, dir: string) => Promise<void>, mergedSha?: string) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-on-land-'));
  const bins = path.join(dir, 'bin'); fs.mkdirSync(bins);
  const home = gitHome(dir);
  stub(bins, 'werk-merge', `echo "merged ${mergedSha ?? SHA}"`);
  if (scopeOut !== null) stub(bins, 'athena-deploy', `echo "athena-deploy $*" >> "${dir}/scope-argv.txt"; printf '%s' "${scopeOut}"`);
  const prev = { bin: process.env.CHORUS_BIN, act: process.env.CHORUS_ACT_BIN, log: process.env.CHORUS_LOG_FILE, home: process.env.CHORUS_HOME };
  process.env.CHORUS_HOME = home;
  process.env.CHORUS_BIN = bins;
  process.env.CHORUS_ACT_BIN = stub(bins, 'act', `echo "act $*" >> "${dir}/act-argv.txt"; sleep 0.2; echo "stub act ran"`);
  process.env.CHORUS_LOG_FILE = path.join(dir, 'chorus.log');
  const server = buildMcpServer(() => 'wren', { runsDir: dir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-on-land-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client, dir); } finally {
    await client.close(); await server.close();
    for (const [k, v] of [['CHORUS_BIN', prev.bin], ['CHORUS_ACT_BIN', prev.act], ['CHORUS_LOG_FILE', prev.log], ['CHORUS_HOME', prev.home]] as const) {
      if (v === undefined) delete process.env[k]; else process.env[k] = v;
    }
  }
}

async function waitFor(file: string, ms = 3000): Promise<string> {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (fs.existsSync(file)) return fs.readFileSync(file, 'utf8');
    await new Promise((r) => setTimeout(r, 50));
  }
  return '';
}

async function merge(client: Client) {
  const res = await client.callTool({ name: 'werk-merge', arguments: { role: 'wren', card_id: 4177 } });
  return JSON.parse((res.content as Array<{ text: string }>)[0].text);
}

test('a land that carried model sources starts the canonical athena run detached, on the landed sha, and the merge reply says so', async () => {
  await withServer('roles/wren/ontology/principles-3749.ttl\n', async (client, dir) => {
    const body = await merge(client);
    assert.equal(body.ok, true, 'the merge is still the merge');
    assert.equal(body.athena.triggered, true);
    assert.equal(body.athena.files, 1);
    assert.ok(fs.existsSync(body.athena.log), 'the detached run has its own log');
    const scopeArgv = fs.readFileSync(path.join(dir, 'scope-argv.txt'), 'utf8');
    assert.ok(scopeArgv.includes(`scope`) && scopeArgv.includes(`${SHA}^..${SHA}`), 'scope asks about exactly the landed commit');
    const argv = await waitFor(path.join(dir, 'act-argv.txt'));
    // #4338 — act ran the landed commit's workflow, never the tree's stale copy
    const wf = /-W (\S+)/.exec(argv)?.[1] ?? '';
    const ran = fs.readFileSync(wf, 'utf8');
    assert.ok(ran.includes(LANDED_MARK) && !ran.includes(TREE_MARK), `act ran the landed athena.yml, got ${wf}: ${ran}`);
    assert.ok(argv.includes('--input target=canonical') && argv.includes(`--input landed_commit=${SHA}`) && argv.includes('--input card_id=4177'));
    const spine = fs.readFileSync(path.join(dir, 'chorus.log'), 'utf8');
    assert.ok(spine.includes('"event":"athena.trigger.started"') && spine.includes(SHA), 'the trigger is witnessed on the spine');
  });
});

test('NEGATIVE PROOF — a land with no model or seed source does not start athena, and says so', async () => {
  await withServer('', async (client, dir) => {
    const body = await merge(client);
    assert.equal(body.ok, true);
    assert.deepEqual(body.athena, { triggered: false, reason: 'no-model-source' });
    await new Promise((r) => setTimeout(r, 300));
    assert.ok(!fs.existsSync(path.join(dir, 'act-argv.txt')), 'act never ran');
    assert.ok(fs.readFileSync(path.join(dir, 'chorus.log'), 'utf8').includes('"event":"athena.trigger.skipped"'));
  });
});

test('NEGATIVE PROOF — a missing scope tool is a named, witnessed trigger failure that never un-lands the merge', async () => {
  await withServer(null, async (client, dir) => {
    const body = await merge(client);
    assert.equal(body.ok, true, 'the code landed; the reply must not lie about that');
    assert.equal(body.athena.triggered, false);
    assert.equal(body.athena.reason, 'scope-failed');
    assert.ok(!fs.existsSync(path.join(dir, 'act-argv.txt')), 'act never ran');
    assert.ok(fs.readFileSync(path.join(dir, 'chorus.log'), 'utf8').includes('"event":"athena.trigger.failed"'));
  });
});

test('NEGATIVE PROOF — a landed sha git cannot show is a named trigger failure, never a run of the tree copy', async () => {
  await withServer('roles/wren/ontology/principles-3749.ttl\n', async (client, dir) => {
    const body = await merge(client);
    assert.equal(body.ok, true, 'the code landed; the reply must not lie about that');
    assert.equal(body.athena.triggered, false);
    assert.equal(body.athena.reason, 'workflow-unreadable');
    await new Promise((r) => setTimeout(r, 300));
    assert.ok(!fs.existsSync(path.join(dir, 'act-argv.txt')), 'act never ran');
  }, 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef');
});
