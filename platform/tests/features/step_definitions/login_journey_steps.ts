// login_journey_steps.ts — #4409. Steps for features/login-journey.feature that
// login_steps.ts does not already have. A leg whose fix has not landed returns
// 'pending'; its scenario carries @waiting-<card> and reads RED by name.
import { Given, When, Then } from '@cucumber/cucumber';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const ROOT = path.resolve(__dirname, '../../../..');
const STEP = { timeout: 60_000 };

// What Kade's idle pane showed at 09:14 on 09-30: the prompt, then Claude
// Code's DIMMED suggestion. Nobody typed it. Until #4362's revert (11:26) pulse
// read it as Jeff typing and held every message to that idle role.
const IDLE_WITH_SUGGESTION = '\u001b[39m❯ \u001b[2m/pull 4393\u001b[0m';

type JourneyWorld = { pane?: string; typed?: string[]; sent?: string; deferred?: boolean };

When('wren goes idle with a grey suggestion on its input line', STEP, function (this: JourneyWorld) {
  this.pane = IDLE_WITH_SUGGESTION;
});

// Pulse's own delivery worker and message store, in-process, on a scratch
// store. The inject records what would be typed into wren's pane. There is no
// pane check left to consult (DEC-107: persist AND deliver, every time), so
// the pane in the world does not change what is typed.
When('Jeff sends wren a message from the Clearing', STEP, async function (this: JourneyWorld) {
  const { MessageStore } = (await import(path.join(ROOT, 'platform/pulse/src/store'))) as { MessageStore: new (p: string) => { sendJeffInput: (to: string, c: string) => number; close?: () => void } };
  const { DeliveryWorker } = (await import(path.join(ROOT, 'platform/pulse/src/delivery-worker'))) as { DeliveryWorker: new (...a: unknown[]) => { enqueue: (r: unknown) => Promise<void> } };
  const db = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'journey-pulse-')), 'messages.db');
  const store = new MessageStore(db);
  const typed: string[] = [];
  let deferred = false;
  const worker = new DeliveryWorker(store, async (_to: string, content: string) => { typed.push(content); return { rc: 0, stderr: '' }; },
    async (event: string) => { if (event.endsWith('.deferred')) deferred = true; }, []);
  this.sent = 'wren can u check the login journey';
  const id = store.sendJeffInput('wren', this.sent);
  await worker.enqueue({ id, from: 'jeff', to: 'wren', content: this.sent, delivery_attempts: 0, kind: 'jeff-input' });
  this.typed = typed;
  this.deferred = deferred;
});

Then("the message is typed into wren's pane at once, whole", STEP, function (this: JourneyWorld) {
  if (this.deferred) throw new Error('pulse deferred the message instead of typing it');
  if (!this.typed || this.typed.length !== 1) throw new Error(`typed ${this.typed?.length ?? 0} times, expected once`);
  if (this.typed[0] !== this.sent) throw new Error(`typed "${this.typed[0]}", expected Jeff's words "${this.sent}"`);
});

// A role's reply reaching Jeff's Clearing (spine reply.delivery.gap
// surface=clearing on 09-21 and 09-30 04:09). The Clearing's own router and
// session tailer, in-process, reading a scratch transcript — the pattern of
// directing/clearing/tests/one-id-4363.test.ts (Silas, navigating).
const CLEARING_TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'journey-clearing-'));
process.env.CLEARING_PROJECTS_DIR = path.join(CLEARING_TMP, 'projects');
process.env.CLEARING_TAILER_OFFSETS = path.join(CLEARING_TMP, 'offsets.json');
process.env.CLEARING_JOURNAL = path.join(CLEARING_TMP, 'room.jsonl');
process.env.CLEARING_REPLY_QUIET_MS = '5';
const WREN_DIR = path.join(process.env.CLEARING_PROJECTS_DIR, '-Users-jeffbridwell-CascadeProjects-chorus-roles-wren');
const TRANSCRIPT = path.join(WREN_DIR, 'sess.jsonl');
const jsonl = (o: object) => JSON.stringify(o) + '\n';

type Tailer = { start: () => void; stop: () => void; checkNow: (role: string) => void };
type Router = { getRecent: (n: number, v: boolean) => Array<{ text: string }> };
type ClearingWorld = { reply?: string; router?: Router; tailer?: Tailer };

// The Clearing is already running when wren answers: its tailer is watching
// wren's transcript (a tailer skips what was written before it started).
When('wren replies', STEP, async function (this: ClearingWorld) {
  const src = path.join(ROOT, 'directing/clearing/src');
  const { MessageRouter } = (await import(path.join(src, 'router'))) as { MessageRouter: new () => Router };
  const { SessionTailer } = (await import(path.join(src, 'session-tailer'))) as { SessionTailer: new (r: unknown, f: () => undefined) => Tailer };
  fs.mkdirSync(WREN_DIR, { recursive: true });
  const ts = new Date().toISOString();
  fs.writeFileSync(TRANSCRIPT, jsonl({ type: 'user', uuid: `u-${Date.now()}`, timestamp: ts, message: { content: 'wren can u check the login journey' } }));
  this.router = new MessageRouter();
  this.tailer = new SessionTailer(this.router, () => undefined);
  this.tailer.start();
  this.reply = 'Checked it: the journey is green up to your Clearing.';
  fs.appendFileSync(TRANSCRIPT, jsonl({ type: 'assistant', uuid: `a-${Date.now()}`, timestamp: ts, message: { content: [{ type: 'text', text: this.reply }], stop_reason: 'end_turn' } }));
});

Then("the reply shows in Jeff's Clearing", STEP, async function (this: ClearingWorld) {
  if (!this.tailer || !this.router) throw new Error('the Clearing was never started');
  this.tailer.checkNow('wren');
  await new Promise((res) => setTimeout(res, 100));
  this.tailer.stop();
  const texts = this.router.getRecent(50, true).map((m) => m.text);
  if (!texts.includes(this.reply ?? '')) {
    throw new Error(`wren's reply never reached the Clearing's room; it holds ${texts.length} message(s): ${JSON.stringify(texts.slice(-3))}`);
  }
});

// The fixture world's scratch dir is private to login_steps.ts; it is the
// newest login-feature-* dir under the OS temp dir (one per scenario).
function world(): string {
  const tmp = os.tmpdir();
  const dirs = fs.readdirSync(tmp).filter((n) => n.startsWith('login-feature-'))
    .map((n) => path.join(tmp, n)).sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
  if (!dirs[0]) throw new Error('no fixture world');
  return dirs[0];
}

Given("wren's session is written down", STEP, function () {
  const T = world();
  const row = JSON.parse(fs.readFileSync(path.join(T, 'identity/wren/session.row.json'), 'utf8')) as { name: string };
  fs.writeFileSync(path.join(T, 'old-session'), `${row.name}\n`);
});

// ---- identity (AC2) ----
// Every run and presence row the login tool wrote, later PUTs applied, so an
// ended run reads ended — what the store would hold.
function rowsOf(T: string, route: string): Array<Record<string, unknown>> {
  const dir = path.join(T, 'bodies');
  const byName = new Map<string, Record<string, unknown>>();
  for (const n of fs.readdirSync(dir).sort()) {
    if (!n.includes(route)) continue;
    const b = JSON.parse(fs.readFileSync(path.join(dir, n), 'utf8')) as Record<string, unknown>;
    const key = String(b.name ?? n.replace(/^.*_/, '').replace(/\.json$/, ''));
    byName.set(key, { ...(byName.get(key) ?? {}), ...b });
  }
  return [...byName.values()];
}

Then('pulse resolves wren to its pane from the Presence row', STEP, async function () {
  const T = world();
  const presences = rowsOf(T, 'identity_presences');
  const runs = rowsOf(T, 'identity_sessionruns');
  type Res = { kind: string };
  const pulse = (await import(path.join(ROOT, 'platform/pulse/src/presence-target'))) as { resolveFromPresence: (p: unknown[], r: unknown[], role: string) => Res };
  const res = pulse.resolveFromPresence(presences, runs, 'wren');
  if (res.kind !== 'resolved') {
    throw new Error(`pulse would answer "logged out — no live Presence": ${JSON.stringify(res)} over ${runs.length} runs, ${presences.length} presences`);
  }
});

Given('the hooks daemon is not answering', STEP, function () {
  const T = world();
  // login's own service list (not the harness stub list), with hooks marked down
  fs.appendFileSync(path.join(T, 'env.extra'), 'unset AWAKE_SERVICES\n');
  fs.writeFileSync(path.join(T, 'down-hooks'), '');
});

Then('the login output says the hooks daemon is down', STEP, function () {
  const out = fs.readFileSync(path.join(world(), 'out'), 'utf8');
  if (!/hooks/i.test(out)) throw new Error(`login never checks the hooks daemon; its output was:\n${out.slice(0, 400)}`);
});

// #4400 names the grant row; until then the setup cannot be written honestly.
Given("wren's Principal row grants it no role", STEP, (): 'pending' => 'pending');
Then('the login is refused, naming the missing role grant, and no session row is written', STEP, (): 'pending' => 'pending');

// ---- the live reply gap ----
type TurnWorld = { router?: Router; tailer?: Tailer; answer?: string; narration?: string };
const assistantLine = (text: string, stop: string) =>
  jsonl({ type: 'assistant', uuid: `a-${Math.random()}`, timestamp: new Date().toISOString(), message: { content: [{ type: 'text', text }], stop_reason: stop } });

Given('Jeff has asked wren something from the Clearing', STEP, async function (this: TurnWorld) {
  const src = path.join(ROOT, 'directing/clearing/src');
  const { MessageRouter } = (await import(path.join(src, 'router'))) as { MessageRouter: new () => Router };
  const { SessionTailer } = (await import(path.join(src, 'session-tailer'))) as { SessionTailer: new (r: unknown, f: () => undefined) => Tailer };
  fs.mkdirSync(WREN_DIR, { recursive: true });
  fs.writeFileSync(TRANSCRIPT, jsonl({ type: 'user', uuid: `u-${Date.now()}`, timestamp: new Date().toISOString(), message: { content: 'wren is the journey green?' } }));
  this.router = new MessageRouter();
  this.tailer = new SessionTailer(this.router, () => undefined);
  this.tailer.start();
});

When('wren writes a line partway through the turn and runs a tool', STEP, async function (this: TurnWorld) {
  this.narration = 'Now the journey, per the navigator.';
  fs.appendFileSync(TRANSCRIPT, assistantLine(this.narration, 'tool_use'));
  this.tailer?.checkNow('wren');
  // the tool runs longer than the Clearing's quiet window
  await new Promise((res) => setTimeout(res, 60));
  fs.appendFileSync(TRANSCRIPT, jsonl({ type: 'user', uuid: `t-${Date.now()}`, timestamp: new Date().toISOString(), message: { content: [{ type: 'tool_result', tool_use_id: 'toolu_x', content: 'ok' }] } }));
  this.tailer?.checkNow('wren');
});

When('wren finishes the turn with its answer', STEP, async function (this: TurnWorld) {
  this.answer = 'Yes, the journey is green end to end.';
  fs.appendFileSync(TRANSCRIPT, assistantLine(this.answer, 'end_turn'));
  this.tailer?.checkNow('wren');
  await new Promise((res) => setTimeout(res, 100));
});

Then("the Clearing shows wren's answer", STEP, function (this: TurnWorld) {
  this.tailer?.stop();
  const texts = (this.router?.getRecent(50, true) ?? []).map((m) => m.text);
  if (!texts.includes(this.answer ?? '')) {
    throw new Error(`the Clearing never showed wren's answer; it shows ${JSON.stringify(texts.slice(-3))}`);
  }
});

Given("silas's nudge is typed into wren's pane", STEP, async function (this: TurnWorld) {
  const src = path.join(ROOT, 'directing/clearing/src');
  const { MessageRouter } = (await import(path.join(src, 'router'))) as { MessageRouter: new () => Router };
  const { SessionTailer } = (await import(path.join(src, 'session-tailer'))) as { SessionTailer: new (r: unknown, f: () => undefined) => Tailer };
  fs.mkdirSync(WREN_DIR, { recursive: true });
  this.router = new MessageRouter();
  this.tailer = new SessionTailer(this.router, () => undefined);
  fs.writeFileSync(TRANSCRIPT, '');
  this.tailer.start();
  // what pulse types since #4362: the message itself, header and all
  fs.appendFileSync(TRANSCRIPT, jsonl({ type: 'user', uuid: `u-${Date.now()}`, timestamp: new Date().toISOString(), message: { content: '[nudge from silas | 2026-09-30 11:50 Boston] is the journey green?' } }));
  this.tailer.checkNow('wren');
});
