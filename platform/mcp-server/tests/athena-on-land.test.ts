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
import { buildMcpServer, athenaDeployFailureNudges, athenaFailedSteps, type FetchImpl } from '../src/server';

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

// #4228 — every POST to pulse's nudge route, as {to, content}. Other reads (the
// word-cap lookup) answer not-ok, which the cap treats as "use the default".
type Sent = { to: string; content: string };
function capturingFetch(sent: Sent[]): FetchImpl {
  return async (url, init) => {
    if (url.includes('/api/nudge') && init?.method === 'POST') {
      const b = JSON.parse(init.body ?? '{}');
      sent.push({ to: b.to, content: b.content });
      return { ok: true, status: 200, json: async () => ({ ok: true }), text: async () => '{"ok":true}' };
    }
    return { ok: false, status: 404, json: async () => ({}), text: async () => '' };
  };
}

async function withServer(scopeOut: string | null, fn: (client: Client, dir: string) => Promise<void>, mergedSha?: string, actExit = 0, sent: Sent[] = []) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-on-land-'));
  const bins = path.join(dir, 'bin'); fs.mkdirSync(bins);
  const home = gitHome(dir);
  stub(bins, 'werk-merge', `echo "merged ${mergedSha ?? SHA}"`);
  if (scopeOut !== null) stub(bins, 'athena-deploy', `echo "athena-deploy $*" >> "${dir}/scope-argv.txt"; printf '%s' "${scopeOut}"`);
  const prev = { bin: process.env.CHORUS_BIN, act: process.env.CHORUS_ACT_BIN, log: process.env.CHORUS_LOG_FILE, home: process.env.CHORUS_HOME };
  process.env.CHORUS_HOME = home;
  process.env.CHORUS_BIN = bins;
  process.env.CHORUS_ACT_BIN = stub(bins, 'act', `echo "act $*" >> "${dir}/act-argv.txt"; sleep 0.2; echo "stub act ran"; exit ${actExit}`);
  process.env.CHORUS_LOG_FILE = path.join(dir, 'chorus.log');
  const server = buildMcpServer(() => 'wren', { runsDir: dir, cardsPath: '/fake/cards', fetchImpl: capturingFetch(sent) });
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

// #4228 reopened (Silas, 2026-10-02): #4338's model deploy failed at 15:54 and the
// only record was a spine line. A failed exit now reaches the landing role and Jeff.
async function waitForSent(sent: Sent[], n: number, ms = 3000): Promise<void> {
  const t0 = Date.now();
  while (sent.length < n && Date.now() - t0 < ms) await new Promise((r) => setTimeout(r, 50));
}

test('a model deploy that exits 1 after the land nudges the landing role and Jeff, naming the card and the log', async () => {
  const sent: Sent[] = [];
  await withServer('roles/wren/ontology/principles-3749.ttl\n', async (_client, dir) => {
    const body = await merge(_client);
    assert.equal(body.athena.triggered, true);
    await waitForSent(sent, 2);
    assert.deepEqual(sent.map((s) => s.to).sort(), ['jeff', 'wren']);
    for (const s of sent) {
      assert.ok(s.content.includes('#4177') && s.content.includes('exit 1'), s.content);
      assert.ok(s.content.includes(body.athena.log), 'the nudge names the log to read');
    }
    assert.ok(fs.readFileSync(path.join(dir, 'chorus.log'), 'utf8').includes('"event":"athena.deploy.failed"'));
  }, undefined, 1, sent);
});

test('NEGATIVE PROOF — a model deploy that exits 0 nudges nobody', async () => {
  const sent: Sent[] = [];
  await withServer('roles/wren/ontology/principles-3749.ttl\n', async (client, dir) => {
    await merge(client);
    const spine = path.join(dir, 'chorus.log');
    const t0 = Date.now();
    while (Date.now() - t0 < 3000 && !fs.readFileSync(spine, 'utf8').includes('athena.deploy.completed')) {
      await new Promise((r) => setTimeout(r, 50));
    }
    assert.ok(fs.readFileSync(spine, 'utf8').includes('"event":"athena.deploy.completed"'), 'the exit was read');
    await new Promise((r) => setTimeout(r, 200));
    assert.deepEqual(sent, []);
  }, undefined, 0, sent);
});

test('the rule alone: exit 0 is nobody; a non-zero exit or a kill is the role and Jeff; a jeff land is one nudge', () => {
  assert.deepEqual(athenaDeployFailureNudges('kade', 7, 0, null, '/l'), []);
  const fail = athenaDeployFailureNudges('kade', 7, 2, null, '/l');
  assert.deepEqual(fail.map((n) => n.to), ['kade', 'jeff']);
  assert.ok(fail[0].message.includes('exit 2') && fail[0].message.includes('/l'));
  assert.ok(athenaDeployFailureNudges('wren', 7, null, 'SIGKILL', '/l')[0].message.includes('killed (SIGKILL)'));
  assert.equal(athenaDeployFailureNudges('jeff', 7, 1, null, '/l').length, 1);
});

// #4467 — the nudge names the step. On #4467's land only prove failed, and the
// nudge said the store did not carry the model, which was false.
test('the nudge names the failed step and says whether the model deployed', () => {
  const log = [
    '[athena/land  ]   ✅  Success - Main deploy [1m18s]',
    '[athena/land  ]   ✅  Success - Main served-after [115ms]',
    '[athena/land  ]   ❌  Failure - Main prove [14m7s]',
  ].join('\n');
  const failed = athenaFailedSteps(log);
  assert.deepEqual(failed, ['prove']);
  const after = athenaDeployFailureNudges('wren', 4467, 1, null, '/l', failed)[0].message;
  assert.ok(after.includes('the prove step failed') && after.includes('model deployed'), after);
  assert.ok(!after.includes('does not carry'), after);
  // negative proof: a failed deploy still says the store does not carry it
  const before = athenaDeployFailureNudges('wren', 4467, 1, null, '/l', athenaFailedSteps('❌  Failure - Main deploy [3s]'))[0].message;
  assert.ok(before.includes('deploy step failed') && before.includes('does not carry'), before);
  // no step in the log: unknown, never a guess either way
  const none = athenaDeployFailureNudges('wren', 4467, 1, null, '/l', [])[0].message;
  assert.ok(none.includes('unknown') && !none.includes('does not carry') && !none.includes('model deployed'), none);
});
