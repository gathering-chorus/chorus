// @test-type: unit — the page's own functions against a hand-built page; no browser, no server.
// @domain: messages
// @card: #4966
// @owner: abby-normal
/**
 * #4966 AC2 — with the sign-in gone, the open page says so without Jeff sending anything, shows
 * nothing of the room, and refuses attach and record with a reason he can see.
 *
 * Jeff, 2026-11-13, walking: "It looks like I'm logged in ... I can't attach a transcription.
 * I can't attach a photo. No errors happen." Measured 2026-11-15: the refusals were written to
 * #send-error, which style.css hides, and nothing told the open page its sign-in had gone.
 */
const app = require('../public/app.js');

const SIGN_IN = 'https://chorus.example/_auth/?next=https%3A%2F%2Fexample%2Fclearing-v2%2F';
const REFUSAL = { ok: false, error: 'not-signed-in', detail: 'Not signed in here.', signInUrl: SIGN_IN };

type El = { id: string; textContent: string; innerHTML: string; className: string; style: { display: string }; children: El[]; href?: string; appendChild(c: El): void };
function el(id: string, text = ''): El {
  const e: El = {
    id, textContent: text, innerHTML: text, className: '', style: { display: 'none' }, children: [],
    appendChild(c: El) { this.children.push(c); },
  };
  return e;
}
function page() {
  const els: Record<string, El> = {
    'messages-stream': el('messages-stream', '<div class="message">a team message</div>'),
    'send-error': el('send-error'),
    'relay-indicator': el('relay-indicator', 'Connected'),
  };
  const doc = {
    getElementById: (id: string) => els[id] || null,
    createElement: (_tag: string) => el(''),
  };
  return { els, doc };
}
const res = (status: number, body: unknown) => ({ status, ok: status >= 200 && status < 300, json: async () => body });

beforeEach(() => app.resetSignedOutState());

describe('#4966 the open page when the sign-in has gone', () => {
  test('a 401 not-signed-in on any call clears the room and shows the notice with the sign-in link', async () => {
    const { els, doc } = page();
    let closed = 0;
    app.setActiveEventSource({ close: () => { closed += 1; } });
    expect(await app.checkSignedOut(res(401, REFUSAL), doc)).toBe(true);
    expect(app.isSignedOut()).toBe(true);
    expect(els['messages-stream'].innerHTML).toBe('');
    expect(els['send-error'].style.display).toBe('block');
    expect(els['send-error'].children[0].href).toBe(SIGN_IN);
    expect(els['send-error'].textContent).toMatch(/signed out/i);
    expect(els['relay-indicator'].textContent).toBe('Signed out');
    expect(closed).toBe(1);
  });

  // NEGATIVE PROOF: with a valid sign-in the same call leaves the room alone and shows no notice
  test('a 200 leaves the room, the notice and the stream untouched', async () => {
    const { els, doc } = page();
    let closed = 0;
    app.setActiveEventSource({ close: () => { closed += 1; } });
    expect(await app.checkSignedOut(res(200, []), doc)).toBe(false);
    expect(app.isSignedOut()).toBe(false);
    expect(els['messages-stream'].innerHTML).toContain('a team message');
    expect(els['send-error'].style.display).toBe('none');
    expect(els['relay-indicator'].textContent).toBe('Connected');
    expect(closed).toBe(0);
  });

  // NEGATIVE PROOF: a 401 that is not the sign-in refusal (or a server error) is not "signed out"
  test.each([[401, { ok: false, error: 'bad-signature' }], [500, { error: 'boom' }], [503, { ok: false, error: 'sign-in-unreadable' }]])(
    'a %i that is not not-signed-in does not clear the room',
    async (status, body) => {
      const { els, doc } = page();
      expect(await app.checkSignedOut(res(status as number, body), doc)).toBe(false);
      expect(els['messages-stream'].innerHTML).toContain('a team message');
    },
  );

  test('once signed out, attach and record are refused before anything is picked or recorded, with a visible reason', () => {
    const { els, doc } = page();
    app.enterSignedOutState(REFUSAL, doc);
    for (const what of ['attach', 'record']) {
      els['send-error'].style.display = 'none';
      expect(app.refuseWhenSignedOut(what, doc)).toBe(true);
      expect(els['send-error'].style.display).toBe('block');
      expect(els['send-error'].textContent).toMatch(new RegExp(`cannot ${what}`, 'i'));
    }
  });

  // NEGATIVE PROOF: signed in, attach and record are not refused and no notice appears
  test('signed in, attach and record go ahead', () => {
    const { els, doc } = page();
    expect(app.refuseWhenSignedOut('attach', doc)).toBe(false);
    expect(app.refuseWhenSignedOut('record', doc)).toBe(false);
    expect(els['send-error'].style.display).toBe('none');
  });
});

describe('#4966 the page asks, it is not only able to', () => {
  const realFetch = globalThis.fetch;
  afterEach(() => { globalThis.fetch = realFetch; });

  // the call the 5 s poll, a dropped stream, waking the phone and a failed attach all make
  test('the sign-in probe on a refused call puts the page in the signed-out state', async () => {
    const asked: string[] = [];
    globalThis.fetch = (async (url: string) => { asked.push(String(url)); return new Response(JSON.stringify(REFUSAL), { status: 401 }); }) as typeof fetch;
    await app.probeSignIn();
    expect(asked[0]).toMatch(/\/api\/tiles$/);
    expect(app.isSignedOut()).toBe(true);
  });

  // NEGATIVE PROOF: a served call, and a network failure, leave the page signed in
  test('a served probe, or one that cannot reach the server, changes nothing', async () => {
    globalThis.fetch = (async () => new Response('[]', { status: 200 })) as typeof fetch;
    await app.probeSignIn();
    expect(app.isSignedOut()).toBe(false);
    globalThis.fetch = (async () => { throw new Error('offline'); }) as typeof fetch;
    await app.probeSignIn();
    expect(app.isSignedOut()).toBe(false);
  });

  test('a real response can still be read by its caller after the check', async () => {
    const r = new Response(JSON.stringify({ ok: false, error: 'bad-signature' }), { status: 401 });
    expect(await app.checkSignedOut(r)).toBe(false);
    expect(await r.json()).toEqual({ ok: false, error: 'bad-signature' });
  });
});

describe('#4966 attach and voice messages are visible', () => {
  test('a status with text is shown; an empty one is hidden again', () => {
    const { els } = page();
    app.showStatus(els['send-error'], '1 of 1 files not attached: Not signed in here.');
    expect(els['send-error'].style.display).toBe('block');
    expect(els['send-error'].textContent).toMatch(/not attached/);
    app.showStatus(els['send-error'], '');
    expect(els['send-error'].style.display).toBe('none');
  });

  test('a status given as html keeps its link and is shown', () => {
    const { els } = page();
    app.showStatus(els['send-error'], '', `<a href="${SIGN_IN}">Sign in</a>`);
    expect(els['send-error'].innerHTML).toContain(SIGN_IN);
    expect(els['send-error'].style.display).toBe('block');
  });
});
