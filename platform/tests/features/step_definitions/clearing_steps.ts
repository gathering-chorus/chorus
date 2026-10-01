// @test-type: bdd — cucumber step definitions; the feature files are the tests
import { Given, When, Then, Before, After, AfterAll } from '@cucumber/cucumber';
import { execSync, spawn, ChildProcess } from 'child_process';
import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const { testClearingEnv, signedInSession } = require('../../../../directing/clearing/tests/lib/test-clearing-world.cjs');
const { spawnSecurityGraphStub } = require('../../../../directing/clearing/tests/lib/security-graph-stub.cjs');
let graph: { url: string; close: () => Promise<void> } | null = null;
const JEFF_WEBID = 'https://pods.example/jeff/profile/card#me';
let jeffSession = '';

// State shared across steps within a scenario
let authToken = '';
let lastResponse = { status: 0, body: '' };
let probeMarker = '';
let nameAccepted = false;

// Endpoints
const LOCAL = 'http://localhost:3470';
// #3366: LAN endpoint is the Bonjour name, never a DHCP-volatile numeric IP
// (the hardcoded .36 went stale when DHCP moved the machine to .23).
const LAN = process.env.CLEARING_LAN_URL || 'http://jeffs-mac-mini-m1-3.local:3470';
const PUBLIC = 'https://clearing.lightlifeurbangardens.com';

// #4417 — sends and feed reads go to a Clearing this file starts, never to
// Jeff's live room. Until 10-01 every nightly stored [e2e-identity] and
// [e2e-test] probes in prod (46 in ~/.chorus/clearing/room.jsonl). The test
// Clearing's world comes from directing/clearing/tests/lib/test-clearing-world.cjs,
// shared with clearing-ui. Page loads and the public door stay live:
// they only read, or are refused.
const TEST_PORT = 12000 + Math.floor(Math.random() * 8000);
const TEST_LOCAL = `http://localhost:${TEST_PORT}`;
const TEST_LAN = LAN.replace(/:\d+$/, `:${TEST_PORT}`);
const CLEARING_SERVER = path.join(__dirname, '..', '..', '..', '..', 'directing', 'clearing', 'dist', 'server.js');
let testWorld = '';
let testToken = '';
let testClearing: ChildProcess | null = null;

Before({ tags: '@clearing', timeout: 20000 }, async function () {
  if (testClearing) return;
  testWorld = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-access-'));
  testToken = `test-${process.pid}-${Date.now()}`;
  // #4417 — Jeff speaks as himself: a signed-in session (his WebID is principal-jeff
  // in the stub graph), never the shared machine credential, which cannot post as a person.
  graph = await spawnSecurityGraphStub({ persons: ['jeff'], principals: { [JEFF_WEBID]: 'principal-jeff' } });
  jeffSession = signedInSession(testWorld, JEFF_WEBID);
  const log: string[] = [];
  testClearing = spawn('node', [CLEARING_SERVER], {
    env: { ...process.env, ...testClearingEnv(testWorld, TEST_PORT, testToken), CHORUS_FUSEKI_QUERY: graph!.url },
    stdio: 'pipe',
  });
  testClearing.stderr?.on('data', (d) => log.push(d.toString()));
  testClearing.on('exit', (code) => log.push(`[exit ${code}]`));
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (curl(`${TEST_LOCAL}/health`).status === 200) return;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`test Clearing did not start on ${TEST_PORT} within 15s: ${log.join('').slice(-400)}`);
});

AfterAll(async function () {
  if (graph) await graph.close();
  if (testClearing && !testClearing.killed) testClearing.kill('SIGTERM');
  if (testWorld) fs.rmSync(testWorld, { recursive: true, force: true });
});

function curl(url: string, opts: string = ''): { status: number; body: string } {
  try {
    const body = execSync(
      `curl -s -o /tmp/clearing-test-body -w '%{http_code}' ${opts} "${url}" --connect-timeout 5 --max-time 10 2>/dev/null`,
      { encoding: 'utf-8', timeout: 15000 }
    ).trim();
    const status = parseInt(body, 10);
    const responseBody = fs.existsSync('/tmp/clearing-test-body')
      ? fs.readFileSync('/tmp/clearing-test-body', 'utf-8')
      : '';
    return { status, body: responseBody };
  } catch (e: any) {
    return { status: 0, body: e.message || 'curl failed' };
  }
}

function curlPost(url: string, data: string, headers: string = ''): { status: number; body: string } {
  try {
    const body = execSync(
      `curl -s -o /tmp/clearing-test-body -w '%{http_code}' -X POST ${headers} -H 'Content-Type: application/json' -d '${data}' "${url}" --connect-timeout 5 --max-time 10 2>/dev/null`,
      { encoding: 'utf-8', timeout: 15000 }
    ).trim();
    const status = parseInt(body, 10);
    const responseBody = fs.existsSync('/tmp/clearing-test-body')
      ? fs.readFileSync('/tmp/clearing-test-body', 'utf-8')
      : '';
    return { status, body: responseBody };
  } catch (e: any) {
    return { status: 0, body: e.message || 'curl failed' };
  }
}

// --- Background ---

Given('the Clearing is running on port {int}', function (port: number) {
  const r = curl(`http://localhost:${port}/health`);
  assert.strictEqual(r.status, 200, `Clearing not running on port ${port}: health returned ${r.status}`);
  const health = JSON.parse(r.body);
  assert.strictEqual(health.status, 'ok', `Clearing unhealthy: ${r.body}`);
});

Given('the auth token is read from ~\\/.chorus\\/bridge-auth-token', function () {
  const tokenPath = `${os.homedir()}/.chorus/bridge-auth-token`;
  assert.ok(fs.existsSync(tokenPath), `Token file missing: ${tokenPath}`);
  authToken = fs.readFileSync(tokenPath, 'utf-8').trim();
  assert.ok(authToken.length > 0, 'Auth token is empty');
});

// --- Page load ---

When('Jeff loads {string} with token cookie', function (url: string) {
  lastResponse = curl(url, `-b "bridge_token=${authToken}" -L`);
});

When('Jeff loads {string} without auth', function (url: string) {
  lastResponse = curl(url, '-L');
});

// #3366: the LAN URL lives in one place (the LAN constant above) so the
// scenario text never carries a DHCP-volatile literal address.
When('Jeff loads the LAN URL without auth', function () {
  lastResponse = curl(LAN, '-L');
});

Then('the page does not return 200', function () {
  assert.notStrictEqual(lastResponse.status, 200, `the shut door served the room: ${lastResponse.body.slice(0, 120)}`);
});

Then('the page returns {int}', function (expectedStatus: number) {
  assert.strictEqual(
    lastResponse.status,
    expectedStatus,
    `Expected ${expectedStatus}, got ${lastResponse.status}. Body preview: ${lastResponse.body.slice(0, 200)}`
  );
});

Then('the page contains {string}', function (text: string) {
  assert.ok(
    lastResponse.body.includes(text),
    `Page does not contain "${text}". Body preview: ${lastResponse.body.slice(0, 300)}`
  );
});

// --- Name entry (identity) ---
// Actor diagram: enter name → join room → set identity → then message

When('Jeff enters the name {string} via the public URL with token auth', function (name: string) {
  const r = curlPost(
    `${PUBLIC}/api/message`,
    JSON.stringify({ from: name, text: `[e2e-identity] ${name} joined` }),
    `-b "bridge_token=${authToken}"`
  );
  nameAccepted = r.status === 200;
  lastResponse = r;
});

When('Jeff enters the name {string} via LAN', function (name: string) {
  // #4278 — /api/message requires a caller identity since #3966; the bridge
  // token is the identity a role carries. Anonymous got 401 (2026-09-23).
  const r = curlPost(
    `${TEST_LAN}/api/message`,
    JSON.stringify({ from: name, text: `[e2e-identity] ${name} joined` }),
    `-b "clearing_session=${jeffSession}"`
  );
  nameAccepted = r.status === 200;
  lastResponse = r;
});

When('Jeff enters the name {string} via localhost', function (name: string) {
  // #4278 — /api/message requires a caller identity since #3966; the bridge
  // token is the identity a role carries. Anonymous got 401 (2026-09-23).
  const r = curlPost(
    `${TEST_LOCAL}/api/message`,
    JSON.stringify({ from: name, text: `[e2e-identity] ${name} joined` }),
    `-b "clearing_session=${jeffSession}"`
  );
  nameAccepted = r.status === 200;
  lastResponse = r;
});

Then('the door does not admit the name', function () {
  // 401 from the host, or a redirect away from the room (308 to the site on
  // 2026-09-23) — either way the token did not get in. 200 would be the defect.
  assert.notStrictEqual(lastResponse.status, 200, `the shut door answered 200: ${lastResponse.body.slice(0, 120)}`);
  assert.ok(!nameAccepted, 'the name must not be accepted through a shut door');
});

Then('the name is accepted', function () {
  assert.ok(nameAccepted, `Name entry failed: status ${lastResponse.status}, body: ${lastResponse.body.slice(0, 200)}`);
});

// --- Message send ---

When('Jeff sends a message {string} via the API with token auth', function (label: string) {
  probeMarker = `[e2e-test] ${label}-${Date.now()}`;
  lastResponse = curlPost(
    `${PUBLIC}/api/message`,
    JSON.stringify({ from: 'jeff', text: probeMarker }),
    `-b "bridge_token=${authToken}"`
  );
  assert.strictEqual(lastResponse.status, 200, `POST failed: ${lastResponse.status} ${lastResponse.body}`);
});

When('Jeff sends a message {string} via the API from LAN', function (label: string) {
  probeMarker = `[e2e-test] ${label}-${Date.now()}`;
  lastResponse = curlPost(
    `${TEST_LAN}/api/message`,
    JSON.stringify({ from: 'jeff', text: probeMarker }),
    `-b "clearing_session=${jeffSession}"` // #4278 — identity is required since #3966
  );
  assert.strictEqual(lastResponse.status, 200, `POST failed: ${lastResponse.status} ${lastResponse.body}`);
});

When('Jeff sends a message {string} via the API from localhost', function (label: string) {
  probeMarker = `[e2e-test] ${label}-${Date.now()}`;
  lastResponse = curlPost(
    `${TEST_LOCAL}/api/message`,
    JSON.stringify({ from: 'jeff', text: probeMarker }),
    `-b "clearing_session=${jeffSession}"` // #4278 — identity is required since #3966
  );
  assert.strictEqual(lastResponse.status, 200, `POST failed: ${lastResponse.status} ${lastResponse.body}`);
});

// --- Message verification ---

Then('the message {string} appears in the message feed', function (_label: string) {
  let found = false;
  for (let i = 0; i < 5; i++) {
    // #4278 — identity is required, and [e2e-…] probes classify as hidden
    // (visible:false) by design so a test never reads as a message to Jeff;
    // the feed must be asked for hidden rows to see its own probe.
    const r = curl(`${TEST_LOCAL}/api/messages?includeHidden=1&limit=2000`, `-H "Authorization: Bearer ${testToken}"`);
    if (r.body.includes(probeMarker)) {
      found = true;
      break;
    }
    execSync('sleep 1');
  }
  assert.ok(found, `Message "${probeMarker}" not found in feed after 5s`);
});

// --- Nudge delivery + role-response steps RETIRED (#2617, 2026-04-30) ---
//
// Retired:
//   - When 'Jeff nudges {word} with {string} via --force'
//   - Then 'the nudge is delivered'
//   - Then '{word} responds via the Clearing within {int} seconds'
//
// Why: these steps invoked real nudges into a live role's session as a side
// effect of running the test, leaking [e2e-test] noise into Jeff's view all
// morning today (~30+ probes traced to manual cucumber runs).
//
// DEC-107's two-path invariant (osascript inject + spine-tick-poller, both
// always fire) makes nudge delivery non-hermetic by design: any code that
// emits a nudge will surface in the target role's view. There is no
// hermetic way to assert "nudge delivered" from cucumber without injecting.
//
// Right shape: this feature scopes to clearing-API behavior (page loads,
// auth, name accept, message send, message in feed) — that's the real test
// value. Nudge delivery has its own tests in
// platform/services/chorus-hooks/tests/nudge_suite.rs (gated behind
// RUN_INTEGRATION per #2614). Role-response e2e probes are a manual
// integration smoke, not a cucumber scenario.

// --- Cleanup ---

After(function () {
  try { fs.unlinkSync('/tmp/clearing-test-body'); } catch { /* ignore */ }
});
