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

  test('NEGATIVE PROOF: note without @role generates zero nudges', () => {
    const msg: ClearingMessage = {
      id: 'note-no-mention',
      from: 'jeff',
      text: 'General broadcast message to the room with no handle',
      ts: new Date().toISOString(),
      type: 'jeff-input',
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
});
