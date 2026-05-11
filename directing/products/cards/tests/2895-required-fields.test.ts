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

const goodDesc = '## Experience\nuser sees X after this lands.\n## AC\n- [ ] thing';
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
