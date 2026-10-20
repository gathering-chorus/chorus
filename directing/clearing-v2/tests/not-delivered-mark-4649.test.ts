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
import { UndeliveredLog } from '../src/undelivered';
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
