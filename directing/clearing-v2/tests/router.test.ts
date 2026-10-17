// @test-type: unit — verifies message routing, deduplication and mention extraction, no live services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { MessageRouter } from '../src/router';
import { ClearingMessage } from '../src/types';

describe('Clearing v2: Message Router & Nudge Dispatch', () => {
  let router: MessageRouter;

  beforeEach(() => {
    router = new MessageRouter();
  });

  test('ingests and stores new messages', () => {
    const msg: ClearingMessage = {
      id: 'note-001',
      from: 'wren',
      text: 'Clearing v2 test note',
      ts: new Date().toISOString(),
      type: 'role-response',
      visible: true,
    };

    const ingested = router.ingest(msg);
    expect(ingested).toBe(true);

    const recent = router.getRecent(10);
    expect(recent.length).toBe(1);
    expect(recent[0].id).toBe('note-001');
    expect(recent[0].from).toBe('wren');
  });

  test('NEGATIVE PROOF: duplicate note ID is dropped and not re-emitted', () => {
    const msg: ClearingMessage = {
      id: 'note-dup-001',
      from: 'silas',
      text: 'Replayed note',
      ts: new Date().toISOString(),
      type: 'role-response',
      visible: true,
    };

    expect(router.ingest(msg)).toBe(true);
    // Ingest exact same ID again
    expect(router.ingest(msg)).toBe(false);

    const recent = router.getRecent(10);
    expect(recent.length).toBe(1);
  });

  test('detects @role mentions and generates Pulse nudge payload', () => {
    const msg: ClearingMessage = {
      id: 'note-mention-001',
      from: 'jeff',
      text: '@wren can you check the test run for #4515?',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    expect(nudges.length).toBe(1);
    expect(nudges[0]).toEqual({
      from: 'jeff',
      to: 'wren',
      content: msg.text,
    });
  });

  test('Jeff note without @role routes to wren ("no @ = Wren gets it", #4571)', () => {
    const msg: ClearingMessage = {
      id: 'note-jeff-no-mention',
      from: 'jeff',
      text: 'Status check on the release',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    expect(nudges).toEqual([
      {
        from: 'jeff',
        to: 'wren',
        content: 'Status check on the release',
      },
    ]);
  });

  test('NEGATIVE PROOF: role note without @role generates zero nudges', () => {
    const msg: ClearingMessage = {
      id: 'note-no-mention',
      from: 'silas',
      text: 'General broadcast message to the room with no handle',
      ts: new Date().toISOString(),
      type: 'role-response',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    expect(nudges.length).toBe(0);
  });

  test('extracts multiple mentions to different roles', () => {
    const msg: ClearingMessage = {
      id: 'note-multi-mention',
      from: 'jeff',
      text: '@silas and @kade please pair on #4522',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    expect(nudges.length).toBe(2);
    expect(nudges.map((n: { to: string }) => n.to).sort()).toEqual(['kade', 'silas']);
  });

  test('#4575: bridge copy + role-signed reply (either order) shows one row under the real author, stripping the prompt header', () => {
    const bridgeCopy: ClearingMessage = {
      id: 'ev-bridge-1',
      from: 'bridge',
      text: 'I filed #4573 and built it.',
      ts: '2026-10-17T10:05:01.000Z',
      type: 'role-response',
      visible: true,
      room: 'team',
    };
    const kadeSigned: ClearingMessage = {
      id: 'ev-kade-1',
      from: 'kade',
      text: '--- Kade | 2026-10-17 06:05 Boston | #4573 | Werk v1.8.1 ---\n\nI filed #4573 and built it.',
      ts: '2026-10-17T10:05:04.000Z',
      type: 'role-response',
      visible: true,
      room: 'team',
    };

    // Order A: bridge arrives first, then kade-signed arrives
    router.ingest(bridgeCopy);
    router.ingest(kadeSigned);
    const recentA = router.getRecent(10);
    expect(recentA).toHaveLength(1);
    expect(recentA[0].from).toBe('kade');
    expect(recentA[0].id).toBe('ev-kade-1');
    expect(recentA[0].text).toBe('I filed #4573 and built it.');

    // Order B: real author (jeff) arrives first, then bridge echo arrives
    const router2 = new MessageRouter();
    router2.ingest({
      id: 'ev-jeff-1',
      from: 'jeff',
      text: '@wren approve',
      ts: '2026-10-17T10:25:40.000Z',
      type: 'jeff-input',
      visible: true,
      room: 'team',
    });
    expect(router2.ingest({
      id: 'ev-bridge-echo-1',
      from: 'bridge',
      text: '@wren approve',
      ts: '2026-10-17T10:25:41.000Z',
      type: 'role-response',
      visible: true,
      room: 'team',
    })).toBe(false);
    const recentB = router2.getRecent(10);
    expect(recentB).toHaveLength(1);
    expect(recentB[0].from).toBe('jeff');
  });

  test('#4575: a v1 message that arrives signed only as bridge is kept, never dropped', () => {
    router.ingest({
      id: 'ev-bridge-only',
      from: 'bridge',
      text: 'Morning from v1 Clearing',
      ts: '2026-10-17T10:20:00.000Z',
      type: 'role-response',
      visible: true,
      room: 'team',
    });
    const recent = router.getRecent(10);
    expect(recent).toHaveLength(1);
    expect(recent[0].text).toBe('Morning from v1 Clearing');
  });
});
