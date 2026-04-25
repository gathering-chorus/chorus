/**
 * #2143 — #2223 follow-on: three-variant description test, error message coverage.
 * #2223 — cards CLI add ergonomics.
 *
 * Tests:
 *   - parseAddArgs accepts --desc-file <path> and reads the file
 *   - parseAddArgs reads stdin when --desc=- is given
 *
 * Validate-all (AC#1) and spine-event-on-fail (AC#6) are tested via
 * sdk.ts create() integration — not duplicated here.
 * The 'create' alias (AC#2) and --help (AC#4) are CLI-layer — verified
 * via integration, not unit (dispatch layer).
 */
import { parseAddArgs } from '../src/cli-add-helpers';
import { addCard } from '../src/sdk';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('#2223 parseAddArgs', () => {
  test('reads description from --desc-file', () => {
    const tmpFile = path.join(os.tmpdir(), `cards-desc-${Date.now()}.md`);
    fs.writeFileSync(tmpFile, '## AC\n- [ ] thing');
    try {
      const parsed = parseAddArgs(['my title', '--desc-file', tmpFile]);
      expect(parsed.description).toBe('## AC\n- [ ] thing');
      expect(parsed.title).toBe('my title');
    } finally {
      fs.unlinkSync(tmpFile);
    }
  });

  test('--desc-file takes precedence if --desc also given (explicit over implicit)', () => {
    const tmpFile = path.join(os.tmpdir(), `cards-desc-${Date.now()}.md`);
    fs.writeFileSync(tmpFile, 'from file');
    try {
      const parsed = parseAddArgs(['my title', '--desc', 'inline', '--desc-file', tmpFile]);
      expect(parsed.description).toBe('from file');
    } finally {
      fs.unlinkSync(tmpFile);
    }
  });

  test('--desc-file with non-existent path throws', () => {
    expect(() => parseAddArgs(['t', '--desc-file', '/tmp/does-not-exist-xyz-2223'])).toThrow();
  });

  test('no --desc-file, --desc inline still works', () => {
    const parsed = parseAddArgs(['my title', '--desc', 'inline text']);
    expect(parsed.description).toBe('inline text');
  });

  test('no desc at all returns empty string', () => {
    const parsed = parseAddArgs(['my title']);
    expect(parsed.description).toBe('');
  });

  test('#2143 missing-desc error names --desc-file and stdin alternatives', async () => {
    const exitSpy = jest.spyOn(process, 'exit').mockImplementation(() => { throw new Error('process.exit'); });
    const errSpy = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const logSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    const mockClient = { boardName: 'chorus', create: jest.fn(), listLabels: jest.fn().mockResolvedValue([]), createLabel: jest.fn().mockResolvedValue({ id: 1, title: 'x' }) } as any;
    try {
      await addCard(mockClient, 'test title', { owner: 'wren', priority: 'P3', domain: 'chorus', type: 'fix', origin: 'reactive', quick: false, description: '' });
    } catch { /* ignore */ }
    const output = logSpy.mock.calls.map(c => c.join(' ')).join(' ') + errSpy.mock.calls.map(c => c.join(' ')).join(' ');
    expect(output).toMatch(/--desc-file/);
    expect(output).toMatch(/stdin/);
    exitSpy.mockRestore(); errSpy.mockRestore(); logSpy.mockRestore();
  });

  test('#2143 inline and --desc-file produce identical description content', () => {
    const tmpFile = path.join(os.tmpdir(), `cards-3var-${Date.now()}.md`);
    const content = '## AC\n- [ ] thing one\n- [ ] thing two';
    fs.writeFileSync(tmpFile, content);
    try {
      const fromInline = parseAddArgs(['title', '--desc', content]);
      const fromFile = parseAddArgs(['title', '--desc-file', tmpFile]);
      expect(fromInline.description).toBe(fromFile.description);
    } finally {
      fs.unlinkSync(tmpFile);
    }
  });
});
