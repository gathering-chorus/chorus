import { Given, When, Then, After } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';

// State shared across steps within a scenario
let authToken = '';
let lastResponse = { status: 0, body: '' };
let probeMarker = '';
let nameAccepted = false;
let lastNudgeOutput = '';

// Endpoints
const LOCAL = 'http://localhost:3470';
const LAN = 'http://192.168.86.36:3470';
const PUBLIC = 'https://clearing.lightlifeurbangardens.com';

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
  const r = curlPost(
    `${LAN}/api/message`,
    JSON.stringify({ from: name, text: `[e2e-identity] ${name} joined` }),
    ''
  );
  nameAccepted = r.status === 200;
  lastResponse = r;
});

When('Jeff enters the name {string} via localhost', function (name: string) {
  const r = curlPost(
    `${LOCAL}/api/message`,
    JSON.stringify({ from: name, text: `[e2e-identity] ${name} joined` }),
    ''
  );
  nameAccepted = r.status === 200;
  lastResponse = r;
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
    `${LAN}/api/message`,
    JSON.stringify({ from: 'jeff', text: probeMarker }),
    ''
  );
  assert.strictEqual(lastResponse.status, 200, `POST failed: ${lastResponse.status} ${lastResponse.body}`);
});

When('Jeff sends a message {string} via the API from localhost', function (label: string) {
  probeMarker = `[e2e-test] ${label}-${Date.now()}`;
  lastResponse = curlPost(
    `${LOCAL}/api/message`,
    JSON.stringify({ from: 'jeff', text: probeMarker }),
    ''
  );
  assert.strictEqual(lastResponse.status, 200, `POST failed: ${lastResponse.status} ${lastResponse.body}`);
});

// --- Message verification ---

Then('the message {string} appears in the message feed', function (_label: string) {
  let found = false;
  for (let i = 0; i < 5; i++) {
    const r = curl(`${LOCAL}/api/messages`);
    if (r.body.includes(probeMarker)) {
      found = true;
      break;
    }
    execSync('sleep 1');
  }
  assert.ok(found, `Message "${probeMarker}" not found in feed after 5s`);
});

// --- Nudge delivery (--force = osascript inject) ---

const NUDGE = '/Users/jeffbridwell/CascadeProjects/platform/scripts/nudge';

When('Jeff nudges {word} with {string} via --force', function (role: string, label: string) {
  const msg = `[e2e-test] ${label}-${Date.now()}`;
  try {
    lastNudgeOutput = execSync(
      `${NUDGE} ${role} "${msg}" --force --from jeff 2>&1`,
      { encoding: 'utf-8', timeout: 10000 }
    ).trim();
  } catch (e: any) {
    lastNudgeOutput = e.stdout || e.stderr || e.message || 'nudge failed';
  }
});

Then('the nudge is delivered', function () {
  assert.ok(
    lastNudgeOutput.includes('DELIVERED'),
    `Nudge not delivered. Output: ${lastNudgeOutput}`
  );
});

// --- Real role response verification ---

Then('{word} responds via the Clearing within {int} seconds', function (role: string, timeout: number) {
  const nudgeMarker = lastNudgeOutput.match(/e2e-[a-z]+-\d+/)?.[0] || '';
  let found = false;
  for (let i = 0; i < timeout; i++) {
    const r = curl(`${LOCAL}/api/messages`);
    if (nudgeMarker && r.body.includes(nudgeMarker) && r.body.includes('[e2e-ack]')) {
      found = true;
      break;
    }
    if (!nudgeMarker && r.body.includes('[e2e-ack]') && r.body.includes(role)) {
      found = true;
      break;
    }
    execSync('sleep 1');
  }
  assert.ok(found, `No [e2e-ack] from ${role} for marker "${nudgeMarker}" in Clearing feed after ${timeout}s. The role's session may not have the e2e-responder hook loaded.`);
});

// --- Cleanup ---

After(function () {
  try { fs.unlinkSync('/tmp/clearing-test-body'); } catch {}
});
