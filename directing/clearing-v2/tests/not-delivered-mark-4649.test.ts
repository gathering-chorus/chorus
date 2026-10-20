// @test-type: unit — page script with a stand-in document, and the server's undelivered log; no browser.
// @card: #4649
// @owner: wren
// @domain: messages
/**
 * #4649 — Jeff 10-19 19:53: "isnt it an issue that u missed one of my first messages
 * completely". The room showed the message as sent while its hand-off to the role had
 * failed; v2 broadcast a 'delivery' failure that the page never handled, and a reload
 * would have lost it anyway.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { UndeliveredLog, DEFAULT_UNDELIVERED_FILE } from '../src/undelivered';
import { resolveUndeliveredFile } from '../src/server';
// @ts-ignore — the page script is plain browser JS with a CommonJS export for tests
import { markUndelivered } from '../public/app.js';

function pageWith(id: string) {
  const msg: any = { id: `msg-${id}`, classList: { added: [] as string[], add(c: string) { this.added.push(c); } }, children: [] as any[],
    appendChild(c: any) { this.children.push(c); }, querySelector: (sel: string) => (sel === '.msg-undelivered' ? msg.children.find((c: any) => c.className === 'msg-undelivered') || null : null) };
  (global as any).document = {
    getElementById: (x: string) => (x === `msg-${id}` ? msg : null),
    createElement: () => ({ className: '', textContent: '' }),
  };
  return msg;
}

describe('#4649 a message that did not reach its role says so on the page', () => {
  afterEach(() => { delete (global as any).document; });

  test('a delivery failure marks the message "not delivered to <role>"', () => {
    const msg = pageWith('n1');
    markUndelivered('n1', ['silas']);
    expect(msg.classList.added).toContain('undelivered');
    expect(msg.children[0].textContent).toBe('not delivered to silas');
  });

  test('a second failed role updates the same mark instead of adding another', () => {
    const msg = pageWith('n2');
    markUndelivered('n2', ['kade']);
    markUndelivered('n2', ['kade', 'wren']);
    expect(msg.children).toHaveLength(1);
    expect(msg.children[0].textContent).toBe('not delivered to kade, wren');
  });

  test('NEGATIVE PROOF: a message with no failure gets no mark', () => {
    const msg = pageWith('n3');
    markUndelivered('other-id', ['silas']);
    expect(msg.children).toHaveLength(0);
    expect(msg.classList.added).toHaveLength(0);
  });
});

describe('#4649 the server remembers failures, so a reload or a second phone still shows them', () => {
  test('failed roles are kept per message, once each', () => {
    const log = new UndeliveredLog();
    expect(log.record('m1', 'silas')).toEqual(['silas']);
    expect(log.record('m1', 'kade')).toEqual(['silas', 'kade']);
    expect(log.record('m1', 'silas')).toEqual(['silas', 'kade']);
    expect(log.all()).toEqual({ m1: ['silas', 'kade'] });
  });

  test('the log stays bounded: the oldest messages drop off past the limit', () => {
    const log = new UndeliveredLog(2);
    log.record('a', 'wren'); log.record('b', 'wren'); log.record('c', 'wren');
    expect(Object.keys(log.all())).toEqual(['b', 'c']);
  });
});

describe('#4649 the list survives a v2 restart (Silas 10-20 05:47)', () => {
  test('a failure written by one v2 is shown by the next one, and the file is replaced atomically', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'undelivered-4649-'));
    const file = path.join(dir, 'undelivered.json');
    new UndeliveredLog(200, file).record('m9', 'silas');
    expect(fs.existsSync(`${file}.tmp`)).toBe(false);
    expect(new UndeliveredLog(200, file).all()).toEqual({ m9: ['silas'] });
  });

  test('NEGATIVE PROOF: with no file, a restart starts empty (the old in-memory behaviour)', () => {
    new UndeliveredLog().record('m9', 'silas');
    expect(new UndeliveredLog().all()).toEqual({});
  });
});

describe('#4649 prod v2 keeps the list on disk; tests never touch ~/.chorus', () => {
  test('prod start (no jest, no override) uses ~/.chorus/clearing-v2-undelivered.json', () => {
    expect(resolveUndeliveredFile(undefined, {})).toBe(DEFAULT_UNDELIVERED_FILE);
    expect(DEFAULT_UNDELIVERED_FILE).toMatch(/\.chorus\/clearing-v2-undelivered\.json$/);
  });
  test('NEGATIVE PROOF: under jest with no path given, nothing is persisted', () => {
    expect(resolveUndeliveredFile(undefined, { JEST_WORKER_ID: '1' })).toBeUndefined();
    expect(resolveUndeliveredFile('/tmp/x.json', { JEST_WORKER_ID: '1' })).toBe('/tmp/x.json');
  });
});

describe('#4649 the red border is styled on the class the page really renders (Wren 10-20 06:06)', () => {
  // The first cut styled `.msg.undelivered`; the page renders `.message`, so the border
  // never showed and the stand-in document above could not tell.
  const css = fs.readFileSync(path.join(__dirname, '../public/style.css'), 'utf8');
  const rendered = /el\.className = '([a-z-]+)';/.exec(fs.readFileSync(path.join(__dirname, '../public/app.js'), 'utf8'))?.[1];
  const hasBorder = (sheet: string, cls: string | undefined) => new RegExp(`\\.${cls}\\.undelivered\\s*\\{[^}]*border-left`).test(sheet);

  test('the message class the page renders has an undelivered border rule', () => {
    expect(rendered).toBe('message');
    expect(hasBorder(css, rendered)).toBe(true);
  });

  test('NEGATIVE PROOF: the old .msg rule does not count as styling the rendered class', () => {
    expect(hasBorder('.msg.undelivered { border-left: 3px solid red; }', rendered)).toBe(false);
  });
});
