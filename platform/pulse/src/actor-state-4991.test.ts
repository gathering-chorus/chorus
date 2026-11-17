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
import { actionFactsFrom, deriveActorState, messageActivity, type ActivityRow, type ActorFactInput } from './actor-state';

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
