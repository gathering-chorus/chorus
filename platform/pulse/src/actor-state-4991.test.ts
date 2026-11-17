// @test-type: unit — activity folded from domain-route rows, then deriveActorState; no live route.
// @domain: roles
// @card: #4991
// @owner: wren
// #4991 (AC4): Activity counts only what a principal did, read from the domain routes
/**
 * #4991 — 11-17 07:21: Teammates said Kade and Silas were "acting" with no session turn; jobs
 * (crawler, deploys, sensors) wrote the model under their names and pulse counted the raw log.
 * Jeff 07:45: "we need to use domain apis to get data here". Activity now reads the streams
 * route (each session's calls and replies) and the messages route (what each one sent).
 * A job's model write is in neither, so it can never make a role read acting.
 */
import { actionFactsFrom, deriveActorState, fetchActivityFacts, messageActivity, streamActivity, type ActivityRow, type ActorFactInput } from './actor-state';

const now = Date.now();
const ago = (s: number) => new Date(now - s * 1000).toISOString();
const KADE = 'principal-kade';
const WREN = 'principal-wren';

const live = (actions: ActorFactInput['actions']): ActorFactInput => ({
  now: new Date(now),
  principals: [KADE, WREN],
  sessions: [KADE, WREN].map((p) => ({ id: `s-${p}`, principal: p, channel: 'pane', lastSeenAt: ago(60), expiresAt: new Date(now + 3_600_000).toISOString() }) as never),
  actions,
});

test('#4991 AC4 a session call and a reply in the streams route make the role acting', () => {
  const rows: ActivityRow[] = [{ role: 'wren', ts: ago(30), what: 'cargo test' }, { role: 'wren', ts: ago(20), what: 'replied (40 words)' }];
  const facts = actionFactsFrom(rows, now);
  expect(facts.get('wren')?.recent5mCount).toBe(2);
  const w = deriveActorState(live(Object.fromEntries(facts))).find((s) => s.aboutPrincipal === WREN);
  expect(w?.activity).toBe('acting');
});

test('#4991 AC4 NEGATIVE PROOF: a role with no rows in the routes (only job writes in the log) reads idle', () => {
  const facts = actionFactsFrom([{ role: 'wren', ts: ago(30), what: 'cargo test' }], now);
  expect(facts.get('kade')).toBeUndefined();
  const k = deriveActorState(live(Object.fromEntries(facts))).find((s) => s.aboutPrincipal === KADE);
  expect(k?.activity).toBe('idle');
});

test('#4991 AC4 old rows count toward the 30-minute window only; a bad time is skipped', () => {
  const facts = actionFactsFrom([{ role: 'kade', ts: ago(20 * 60), what: 'x' }, { role: 'kade', ts: 'not a time', what: 'y' }], now);
  expect(facts.get('kade')).toMatchObject({ recent30mCount: 1 });
  expect(facts.get('kade')?.recent5mCount).toBeUndefined();
});

test('#4991 AC4 a message the role sent counts; NEGATIVE PROOF: an alert to the role (sender is also recipient) does not', () => {
  const sent = { senderName: 'wren', sentBy: 'principal-wren', sentTo: ['principal-abby-normal'], sentAt: ago(10) };
  const alert = { senderName: 'kade', sentBy: 'principal-kade', sentTo: ['principal-kade'], sentAt: ago(10), messageBody: '[alert for kade · from system | ...]' };
  expect(messageActivity(sent)).toEqual({ role: 'wren', ts: sent.sentAt, what: 'message' });
  expect(messageActivity(alert)).toBeNull();
  expect(messageActivity({ senderName: 'system', sentBy: 'system', sentTo: ['principal-kade'], sentAt: ago(10) })).toBeNull();
  const k = deriveActorState(live(Object.fromEntries(actionFactsFrom([messageActivity(alert)].filter((r): r is ActivityRow => r !== null), now)))).find((s) => s.aboutPrincipal === KADE);
  expect(k?.activity).toBe('idle');
});

// #4991 demo review (code) — a reply is not work (#4766), and a running call is acting now
test('a reply row is no activity; a call still running counts as now, however old its start', () => {
  expect(streamActivity({ streamRole: 'kade', streamPhase: 'reply', streamTs: ago(10) })).toBeNull();
  const nowIso = new Date(now).toISOString();
  expect(streamActivity({ streamRole: 'kade', streamPhase: 'running', streamTs: ago(3600) }, nowIso)?.ts).toBe(nowIso);
  expect(streamActivity({ streamRole: 'kade', streamPhase: 'done', streamTs: ago(3600) }, nowIso)?.ts).toBe(ago(3600));
});

test('system senders are skipped whatever their case', () => {
  expect(messageActivity({ senderName: 'System', sentBy: 'x', sentTo: ['y'], sentAt: ago(10) })).toBeNull();
});

// NEGATIVE PROOF (#4991 demo review): an unread activity route reads unmeasured, never idle
test('NEGATIVE PROOF: with the streams route unread, a quiet role reads unmeasured, not idle', () => {
  const read = deriveActorState(live({}));
  const unread = deriveActorState({ ...live({}), unmeasured: [{ source: 'pulse:streams', reason: 'HTTP 502' }] });
  const kade = (r: typeof read) => r.find((x) => x.aboutPrincipal === KADE)?.activity;
  expect(kade(read)).toBe('idle');
  expect(kade(unread)).toBe('unmeasured');
});

// #4991 demo review (code) — a demo pulse (PULSE_API_URL=off) never reads prod's routes
test('with the pulse API off, activity reads nothing and says it is unmeasured', async () => {
  const unmeasured: Array<{ source: string; reason: string }> = [];
  const got = await fetchActivityFacts('off', unmeasured);
  expect(got.size).toBe(0);
  expect(unmeasured.map((u) => u.source)).toEqual(['pulse:streams']);
});
