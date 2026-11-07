// @test-type: unit — pure: which messages stay out of Jeff's room and out of his name.
// @card: #4875
// @owner: wren
// @domain: messages
// AC10 (Jeff 2026-11-03 09:36 screenshot): the owner's demo brief showed in #team twice, once as
// "system [alert → wren]" and once as a message from jeff (the terminal hook recorded the alert).
import { ownerOnly, injectedDelivery } from './buzz-writer';

const brief = 'Demo brief for the owner — #4875 · round e49d23e76ede · gates ✓ · for you, not Jeff: show it working';

describe('#4875 AC10: owner briefs stay out of Jeff\'s room and out of his name', () => {
  test('a demo brief from system is owner-only, with or without its alert header', () => {
    expect(ownerOnly({ from: 'system', content: brief })).toBe(true);
    expect(ownerOnly({ from: 'system', content: `[alert for wren · from system | 2026-11-03 09:35 Boston] ${brief}` })).toBe(true);
  });

  test('negative proof: other alerts, and anyone quoting the words, still reach the room', () => {
    expect(ownerOnly({ from: 'system', content: '[alert for kade] crawler: 16 domains failing' })).toBe(false);
    expect(ownerOnly({ from: 'wren', content: brief })).toBe(false);
  });

  test('an alert or nudge a terminal received is not recorded as Jeff words', () => {
    expect(injectedDelivery(`[alert for wren · from system | 2026-11-03 09:35 Boston] ${brief}`)).toBe(true);
    expect(injectedDelivery('[nudge from kade | 2026-11-03 09:37 Boston] Good.')).toBe(true);
  });

  test('negative proof: what Jeff types, even about alerts, is still his', () => {
    expect(injectedDelivery('@wren fix it now')).toBe(false);
    expect(injectedDelivery('why do i see [alert for wren] in the room')).toBe(false);
  });
});
