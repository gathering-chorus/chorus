/**
 * #2895 — proposal store unit tests.
 *
 * Exercises the in-memory ProposalStore lifecycle: submit → pending → approve/deny.
 */
import { proposalStore } from '../src/card-proposals';

const baseInput = () => ({
  role: 'wren', title: 'test', owner: 'wren', priority: 'P2',
  domain: 'chorus', type: 'fix', origin: 'reactive', sequence: 'chorus',
  description: '## Experience\nx\n## AC\n- [ ] thing',
});

describe('#2895 proposalStore lifecycle', () => {
  test('proposalStore.submit returns a proposal with status pending and a UUID', () => {
    const p = proposalStore.submit({ ...baseInput(), title: 'submit-test' });
    expect(p.status).toBe('pending');
    expect(p.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(p.title).toBe('submit-test');
    expect(p.submittedAt).toBeLessThanOrEqual(Date.now());
  });

  test('proposalStore.approve transitions pending to approved', async () => {
    const p = proposalStore.submit({ ...baseInput(), title: 'approve-me' });
    const after = await proposalStore.approve(p.id);
    expect(after?.status).toBe('approved');
    expect(after?.decidedAt).toBeDefined();
  });

  test('proposalStore.deny captures the reason', () => {
    const p = proposalStore.submit({ ...baseInput(), title: 'deny-me' });
    const after = proposalStore.deny(p.id, 'not aligned');
    expect(after?.status).toBe('denied');
    expect(after?.deniedReason).toBe('not aligned');
  });

  test('proposalStore.approve on a non-pending proposal is a no-op', async () => {
    const p = proposalStore.submit({ ...baseInput(), title: 'decided' });
    await proposalStore.approve(p.id);
    const after = proposalStore.deny(p.id, 'too late');
    expect(after?.status).toBe('approved');
    expect(after?.deniedReason).toBeUndefined();
  });

  test('proposalStore.pending returns only pending proposals sorted by submission time', () => {
    const before = proposalStore.pending().length;
    const a = proposalStore.submit({ ...baseInput(), title: 'a' });
    const b = proposalStore.submit({ ...baseInput(), title: 'b' });
    proposalStore.deny(a.id, 'no');
    const list = proposalStore.pending();
    expect(list.length).toBe(before + 1);
    expect(list[list.length - 1].id).toBe(b.id);
  });

  test('proposalStore.get returns the proposal by id', () => {
    const p = proposalStore.submit({ ...baseInput(), title: 'lookup' });
    const fetched = proposalStore.get(p.id);
    expect(fetched?.title).toBe('lookup');
  });

  test('proposalStore.get returns undefined for unknown id', () => {
    expect(proposalStore.get('00000000-0000-0000-0000-000000000000')).toBeUndefined();
  });
});
