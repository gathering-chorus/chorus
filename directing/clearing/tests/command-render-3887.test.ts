// @test-type: unit — pure text normalization; brings its own world.
/**
 * #3887 — Jeff's slash commands render as commands, not as harness markup.
 *
 * He has named this three times. The row he was shown today, verbatim from the
 * live room:
 *
 *   <command-message>fuc</command-message> <command-name>/fuc</command-name> <command-args>!!!
 *
 * NEGATIVE PROOF (#3734): `the exact row Jeff was shown` feeds that string in
 * and requires the envelope GONE. It fails against the old code, where the
 * single-string branch returned `rawContent.trim()` untouched — the array path
 * had handled commands since #3852 and the string path silently bypassed it.
 * Every other test here would pass on the broken code, because they only assert
 * what the good path already did.
 */
import { normalizeCommandText } from '../src/session-tailer';

describe('#3887 command rendering', () => {
  it('NEGATIVE: the exact row Jeff was shown loses its envelope', () => {
    const shown = '<command-message>fuc</command-message> <command-name>/fuc</command-name> <command-args>!!!</command-args>';
    const out = normalizeCommandText(shown);
    expect(out).toBe('/fuc !!!');
    expect(out).not.toContain('<command-');
    expect(out).not.toContain('command-message');
  });

  it('a command with no arguments renders as just the command', () => {
    expect(normalizeCommandText('<command-message>wip</command-message> <command-name>/wip</command-name> <command-args></command-args>'))
      .toBe('/wip');
  });

  it('multi-line envelopes are handled — the harness does not promise one line', () => {
    const raw = [
      '<command-message>close</command-message>',
      '<command-name>/close</command-name>',
      '<command-args>wrapping up for today</command-args>',
    ].join('\n');
    expect(normalizeCommandText(raw)).toBe('/close wrapping up for today');
  });

  it('ordinary typing is returned untouched', () => {
    expect(normalizeCommandText('  seems like u stopped  ')).toBe('seems like u stopped');
  });

  it('a message that legitimately quotes XML is NOT stripped', () => {
    // The tempting fix is to strip anything angle-bracketed. That would eat this
    // sentence, and Jeff quotes markup at us regularly.
    const quoted = 'the room shows <command-message> raw and it looks broken';
    expect(normalizeCommandText(quoted)).toBe(quoted);
  });

  it('an unterminated args tag still yields the command, not the markup', () => {
    // Exactly the shape in the live row: the text was cut mid-tag.
    const truncated = '<command-message>fuc</command-message> <command-name>/fuc</command-name> <command-args>!!!';
    const out = normalizeCommandText(truncated);
    expect(out).toBe('/fuc');
    expect(out).not.toContain('<');
  });
});
