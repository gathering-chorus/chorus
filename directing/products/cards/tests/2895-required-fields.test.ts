/**
 * #2895 — cards CLI hard hook: required-field promotion.
 *
 * Promotes 'No --sequence' and 'No "## Experience" section' from WARN to ERROR.
 * Previously these warned but did not fail validation, which drove a multi-turn
 * add dance: agent retries after seeing the WARN as a fix-able thing.
 *
 * Tests call addCard directly with a mock client and capture the exit/stderr.
 * Same pattern as the #2143 missing-desc test in cli-add-ergonomics.test.ts.
 */
import { addCard } from '../src/sdk';

function mockClient() {
  return {
    boardName: 'chorus',
    create: jest.fn(),
    listLabels: jest.fn().mockResolvedValue([]),
    createLabel: jest.fn().mockResolvedValue({ id: 1, title: 'x' }),
    add: jest.fn(),
    tag: jest.fn(),
    view: jest.fn(),
    comment: jest.fn(),
  } as any;
}

function spies() {
  const exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => { throw new Error('process.exit'); });
  const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
  const logSpy = jest.spyOn(console, 'log').mockImplementation(() => {});
  return {
    exitSpy, errSpy, logSpy,
    output: () => errSpy.mock.calls.map(c => c.join(' ')).join(' ') + ' ' + logSpy.mock.calls.map(c => c.join(' ')).join(' '),
    restore: () => { exitSpy.mockRestore(); errSpy.mockRestore(); logSpy.mockRestore(); },
  };
}

// #2905: six required sections in description, each with substance.
const goodWhy = '## Why this matters\nWithout this fix the X path silently drops events that downstream consumers depend on, and the Y page renders stale data until restart. This affects normal usage weekly for multiple roles plus Jeff during active sessions, not an edge case.';
const goodHelpsChorus = '## Why it helps Chorus\nThe spine becomes signal-rich for all three roles plus Jeff. Direct improvement to chorus_logs_for_card joins and recent-errors queries, the surface the whole team reads when something breaks during a card.';
const goodNotGoldPlating = "## Why it's not gold plating or a nit\nThe noise is currently masking real failure modes; I missed two yesterday because tunnel-probe events drowned the signal. Cosmetic would be reformatting; this is recovering load-bearing observability for the team.";
const goodDeps = '## Dependencies\nNone external. One file in chorus-hooks observer.rs; ships with the same build pipeline as the rest of the hook crate. No coordination required with other roles.';
const goodScope = '## Scope of impact\nEvery observer.error emit and every chorus_logs_for_card query touch this. All three agent roles plus Jeff read the resulting spine surface. Tested with a real cf-connecting-ip fixture; no API shape change.';
const goodDesc = `## Experience\nuser sees X after this lands.\n${goodWhy}\n${goodHelpsChorus}\n${goodNotGoldPlating}\n${goodDeps}\n${goodScope}\n## AC\n- [ ] thing`;
const fullOpts = {
  owner: 'wren', priority: 'P2', domain: 'chorus', type: 'fix',
  origin: 'reactive', sequence: 'chorus', description: goodDesc, quick: false,
};

describe('#2895 promotes WARN to ERROR for sequence and Experience', () => {
  test('missing --sequence now fails with a sequence error', async () => {
    const s = spies();
    const opts = { ...fullOpts, sequence: '' };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* expected exit */ }
    const out = s.output();
    expect(out).toMatch(/missing\s+--sequence/i);
    s.restore();
  });

  test('--quick still bypasses the sequence requirement', async () => {
    const s = spies();
    const opts = { ...fullOpts, sequence: '', quick: true, description: '' };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* may or may not exit */ }
    const out = s.output();
    expect(out).not.toMatch(/missing.*sequence/i);
    s.restore();
  });

  test('missing Experience section now fails with an experience error', async () => {
    const s = spies();
    const noExp = '## AC\n- [ ] thing';
    const opts = { ...fullOpts, description: noExp };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* expected exit */ }
    const out = s.output();
    expect(out).toMatch(/description missing.*experience/i);
    s.restore();
  });

  test('--quick still bypasses the Experience requirement', async () => {
    const s = spies();
    const opts = { ...fullOpts, description: '', quick: true };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* may exit on other errors */ }
    const out = s.output();
    expect(out).not.toMatch(/missing.*experience/i);
    s.restore();
  });

  test('#2905: missing "## Why this matters" section is refused', async () => {
    const s = spies();
    const noWhy = '## Experience\nx\n## AC\n- [ ] thing';
    const opts = { ...fullOpts, description: noWhy };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* expected exit */ }
    const out = s.output();
    expect(out).toMatch(/why\s+this\s+matters/i);
    s.restore();
  });

  test('#2905: thin "## Why this matters" (<30 words) is refused as a nit', async () => {
    const s = spies();
    const thinWhy = '## Experience\nx\n## Why this matters\nIt matters because it does.\n## AC\n- [ ] thing';
    const opts = { ...fullOpts, description: thinWhy };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* expected exit */ }
    const out = s.output();
    expect(out).toMatch(/too thin|nit|substantive/i);
    s.restore();
  });

  test('#2905: substantive "## Why this matters" (≥30 words) passes the section check', async () => {
    const s = spies();
    try { await addCard(mockClient(), 'test title', fullOpts); } catch { /* shouldn't exit on validation */ }
    const out = s.output();
    expect(out).not.toMatch(/missing.*why\s+this\s+matters/i);
    expect(out).not.toMatch(/why\s+this\s+matters.*too\s+thin/i);
    s.restore();
  });

  test('#2905: CHORUS_BYPASS_PROPOSAL=1 from agent shell has no effect on validation refusal', async () => {
    const s = spies();
    const prior = process.env.CHORUS_BYPASS_PROPOSAL;
    process.env.CHORUS_BYPASS_PROPOSAL = '1';
    try {
      const noWhy = '## Experience\nx\n## AC\n- [ ] thing';
      const opts = { ...fullOpts, description: noWhy };
      try { await addCard(mockClient(), 'test title', opts); } catch { /* expected */ }
      const out = s.output();
      expect(out).toMatch(/why\s+this\s+matters/i);
    } finally {
      if (prior === undefined) delete process.env.CHORUS_BYPASS_PROPOSAL;
      else process.env.CHORUS_BYPASS_PROPOSAL = prior;
      s.restore();
    }
  });

  test('all required fields in one refusal — multi-missing prints all errors together', async () => {
    const s = spies();
    const opts = { owner: '', priority: '', domain: '', type: '', origin: '', sequence: '', description: '', quick: false };
    try { await addCard(mockClient(), 'test title', opts); } catch { /* expected exit */ }
    const out = s.output();
    expect(out).toMatch(/domain/i);
    expect(out).toMatch(/type/i);
    expect(out).toMatch(/priority/i);
    expect(out).toMatch(/origin/i);
    expect(out).toMatch(/missing\s+--sequence/i);
    s.restore();
  });
});
