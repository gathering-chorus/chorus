// @test-type: unit — in-memory router, no live services.
/**
 * #3862 (third leg) — harness XML must not render as chat.
 *
 * Jeff's phone, 2026-08-13 18:19, sent as a seed. In his OWN blue bubble:
 *
 *   <task-notification> <task-id>bt5jao0v1</task-id>
 *   <tool-use-id>toolu_018C27GNsUiZT197BehyNq6S</tool-use-id>
 *   <output-file>/private/tmp/claude-501/…/tasks/bt5jao0v1.output</output-file>
 *   <status>completed</status> …
 *
 * This is a regression I caused earlier in this same card. The old noise sweep
 * hid anything matching /<[a-z-]+>/ — which covered these — and I narrowed that
 * sweep to non-role senders so replies would stop vanishing. Harness
 * notifications arrive attributed to jeff, so they walked through the gap.
 *
 * That is the cost of the first fix stated honestly: it traded hidden replies
 * for machinery rendered as speech. The machinery list is the right home,
 * because it applies to EVERY sender, including Jeff.
 *
 * NEGATIVE PROOF (#3734): the first two cases assert the opposite of current
 * behaviour and fail before the fix. The last two exist because the obvious
 * over-correction — filter anything containing an angle bracket — would silence
 * a person writing "a < b" or quoting a tag, and this suite must be able to
 * tell those apart.
 */

import { MessageRouter } from '../src/router';

const TS = '2026-08-13T22:19:00Z';

describe('#3862 — harness notifications are not chat', () => {
  it('keeps a task-notification out of the room, even attributed to Jeff', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'jeff',
      text: '<task-notification> <task-id>bt5jao0v1</task-id> <status>completed</status> </task-notification>',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  it('keeps one from a role out too', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'kade',
      text: '<task-notification><tool-use-id>toolu_018C27</tool-use-id><summary>Background command completed (exit code 0)</summary></task-notification>',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  it('does NOT filter a person using an angle bracket in prose', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'jeff', text: 'keep the reply < 100 words please', ts: TS });
    expect(r.getRecent(10)[0].visible).toBe(true);
  });

  it('does NOT filter a reply that merely mentions the tag by name', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'wren',
      text: 'The task-notification blobs were rendering as your own messages — fixed.',
      ts: TS,
    });
    expect(r.getRecent(10)[0].visible).toBe(true);
  });
});
