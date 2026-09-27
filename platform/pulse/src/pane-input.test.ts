// @test-type: unit — pure parse of a captured pane; no tmux, no store
/**
 * #4362 — the wake line waits while Jeff is typing.
 *
 * Captures below are shaped like a real Claude Code pane (the input line is
 * "❯" + NBSP, drawn between two box rules). Jeff's words on that line mean
 * a keystroke from pulse would land in the middle of his prompt.
 */
import { paneHasTypedInput } from './pane-input';

const RULE = '─'.repeat(40);
const pane = (inputLine: string, extra = '') =>
  ['some earlier output', '', RULE, inputLine, RULE, '  auto mode on (shift+tab to cycle)', extra].join('\n');

describe('paneHasTypedInput (#4362)', () => {
  test('an empty input line is not typing', () => {
    expect(paneHasTypedInput(pane('❯\u00a0'))).toBe(false);
  });

  test('a plain-space empty input line is not typing', () => {
    expect(paneHasTypedInput(pane('❯ '))).toBe(false);
  });

  // Negative proof (#3734): the same pane with Jeff's half-typed words must
  // read as typing. Delete the text check in pane-input.ts and this goes red.
  test('half-typed words on the input line are typing', () => {
    expect(paneHasTypedInput(pane('❯\u00a0how do we host my experience more de'))).toBe(true);
  });

  test('ANSI colour codes around the prompt do not hide the words', () => {
    expect(paneHasTypedInput(pane('\u001b[38;5;246m❯\u00a0\u001b[39mif we have all our edges'))).toBe(true);
    expect(paneHasTypedInput(pane('\u001b[38;5;246m❯\u00a0\u001b[39m'))).toBe(false);
  });

  test('a ❯ in earlier output does not count; only the last input line does', () => {
    const cap = ['❯ old prompt text from scrollback', RULE, '❯\u00a0', RULE].join('\n');
    expect(paneHasTypedInput(cap)).toBe(false);
  });

  test('no input line found (not a Claude pane) is not typing', () => {
    expect(paneHasTypedInput('bash-3.2$ ')).toBe(false);
    expect(paneHasTypedInput('')).toBe(false);
  });
});
