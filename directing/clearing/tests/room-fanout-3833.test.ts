// @test-type: unit — imports the target picker and calls it; no server, no sockets, no pulse.
/**
 * #3833 — who hears Jeff.
 *
 * The Clearing was a switchboard: an unaddressed message went to exactly one
 * role, and that role was always Wren. Jeff has asked since 2026-03-08 for "a
 * team interaction not just a jeff->team style", and the transport was never
 * what stopped it — this function was.
 *
 * The trap in testing this: an assertion like "wren is among the targets"
 * passes under the OLD behaviour and the new one, and proves nothing. So the
 * tests here pin the difference, in both directions.
 */

import { pickJeffMessageTargets } from '../src/server';

describe('#3833 an unaddressed message reaches the room', () => {
  test('no mention → all three roles', () => {
    expect(pickJeffMessageTargets('morning').sort()).toEqual(['kade', 'silas', 'wren']);
  });

  test('NEGATIVE PROOF: the old behaviour is gone, not merely joined', () => {
    // The retired rule, asserted as retired. If someone restores the
    // single-target default, this fails loudly instead of the room quietly
    // shrinking back to one listener — which is exactly how it would be
    // reintroduced, since everything still "works" from Wren's side.
    expect(pickJeffMessageTargets('morning')).not.toEqual(['wren']);
    expect(pickJeffMessageTargets('morning')).toHaveLength(3);
  });

  test('a question with no @ still reaches everyone', () => {
    expect(pickJeffMessageTargets('what happened to the nightly?')).toHaveLength(3);
  });
});

describe('#3833 addressing still means something', () => {
  test('@role targets exactly that role', () => {
    expect(pickJeffMessageTargets('@silas can you look at the relay')).toEqual(['silas']);
  });

  test('NEGATIVE PROOF: an addressed message does NOT fan out', () => {
    // If the change made everything go everywhere, addressing would be
    // meaningless and the room unusable — you could never take something aside.
    // This is the assertion that stops "fix the room" from becoming "shout".
    const targets = pickJeffMessageTargets('@kade the suite hangs after 567');
    expect(targets).toEqual(['kade']);
    expect(targets).not.toContain('wren');
    expect(targets).not.toContain('silas');
  });

  test('two mentions target exactly those two', () => {
    expect(pickJeffMessageTargets('@wren @silas pair on this').sort()).toEqual(['silas', 'wren']);
  });

  test('the same role mentioned twice is delivered once', () => {
    expect(pickJeffMessageTargets('@wren and again @wren')).toEqual(['wren']);
  });

  test('mentions are case-insensitive', () => {
    expect(pickJeffMessageTargets('@Silas look')).toEqual(['silas']);
  });
});

describe('#3833 unrecognised mentions', () => {
  test('@team reaches the room rather than one role by accident', () => {
    // The bug Jeff hit on 2026-08-11: he posted an article to "@team" and it
    // reached Wren alone, silently. It looked like a broadcast and behaved
    // like a DM. Now an unrecognised mention falls through to everyone, which
    // is what he meant.
    expect(pickJeffMessageTargets('@team read this').sort()).toEqual(['kade', 'silas', 'wren']);
  });

  test('NEGATIVE PROOF: @team does not resolve to wren alone', () => {
    expect(pickJeffMessageTargets('@team read this')).not.toEqual(['wren']);
  });

  test('a bare @ or an unknown name does not silence the room', () => {
    expect(pickJeffMessageTargets('@ everyone')).toHaveLength(3);
    expect(pickJeffMessageTargets('@nobody hello')).toHaveLength(3);
  });
});
