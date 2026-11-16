// @test-type: integration:ui
// @card: #4866
// @owner: abby-normal
// @domain: messages
// also proves #4966 (AC2, the open page when the sign-in has gone): its cases are the last block in this file
/**
 * #4866 AC4 — Clear auth error guidance when session cookie is absent on mobile upload/voice calls.
 * 401 returns not-signed-in with signInUrl and clear detail; UI renders actionable sign-in link.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';

const DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-v2-upload-auth-4866-'));
process.env.CLEARING_V2_UPLOAD_DIR = DIR;
const { createApp } = require('../src/server');
const { uploadPhoto, uploadFile, formatUploadError } = require('../public/app.js');

const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
const SIGN_IN_URL = 'https://chorus.lightlifeurbangardens.com/_auth/?next=https%3A%2F%2Flightlifeurbangardens.com%2Fclearing-v2%2F';

async function up() {
  const { app, relay } = createApp({
    relayUrl: 'ws://127.0.0.1:1',
    pulseUrl: 'http://127.0.0.1:9',
    cookieKey: KEY,
    signInUrl: SIGN_IN_URL
  });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(server.address() as any).port}`;
  return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
}

describe('#4866 AC4: upload auth error guidance', () => {
  test('missing session cookie returns 401 with signInUrl and descriptive detail on /api/upload', async () => {
    const s = await up();
    try {
      const res = await fetch(`${s.base}/api/upload`, {
        method: 'POST',
        headers: { 'Content-Type': 'image/png' },
        body: Buffer.from('fake-image')
      });
      expect(res.status).toBe(401);
      const body = await res.json() as { ok: boolean; error: string; detail: string; signInUrl: string };
      expect(body.ok).toBe(false);
      expect(body.error).toBe('not-signed-in');
      expect(body.signInUrl).toBe(SIGN_IN_URL);
      expect(body.detail).toMatch(/Sign in at/);
    } finally { await s.close(); }
  });

  test('missing session cookie returns 401 with signInUrl on /api/voice', async () => {
    const s = await up();
    try {
      const res = await fetch(`${s.base}/api/voice`, {
        method: 'POST',
        headers: { 'Content-Type': 'audio/webm' },
        body: Buffer.from('fake-audio')
      });
      expect(res.status).toBe(401);
      const body = await res.json() as { ok: boolean; error: string; signInUrl: string };
      expect(body.ok).toBe(false);
      expect(body.error).toBe('not-signed-in');
      expect(body.signInUrl).toBe(SIGN_IN_URL);
    } finally { await s.close(); }
  });

  test('formatUploadError provides actionable HTML with signInUrl link when not signed in', () => {
    const errorJson = {
      ok: false,
      error: 'not-signed-in',
      detail: 'Not signed in here.',
      signInUrl: SIGN_IN_URL
    };
    const rendered = formatUploadError(errorJson);
    expect(rendered.isAuth).toBe(true);
    expect(rendered.signInUrl).toBe(SIGN_IN_URL);
    expect(rendered.html).toContain(`<a href="${SIGN_IN_URL}"`);
  });

  test('watchFileButton renders tappable signInUrl link in send-error status when upload returns 401', async () => {
    const { watchFileButton } = require('../public/app.js');
    let onChange: () => Promise<void> = async () => {};
    const picker = {
      files: [{ name: 'test.jpg', type: 'image/jpeg' }],
      value: 'chosen',
      addEventListener: (_e: string, f: () => Promise<void>) => { onChange = f; }
    };
    const input = { value: '', focus: () => {} };
    const status = { innerHTML: '', textContent: '' };
    (global as any).document = {
      getElementById: (id: string) => ({ 'composer-file': picker, 'composer-input': input, 'send-error': status } as any)[id] || null,
      querySelector: () => null
    };
    (global as any).fetch = async () => ({
      ok: false,
      status: 401,
      json: async () => ({
        ok: false,
        error: 'not-signed-in',
        detail: 'Not signed in here.',
        signInUrl: SIGN_IN_URL
      })
    });

    watchFileButton();
    await onChange();

    expect(status.innerHTML).toContain(`<a href="${SIGN_IN_URL}">Sign in to send</a>`);
    delete (global as any).document;
    delete (global as any).fetch;
  });
});

// ==========================================================================================
// #4966 — AC2, the open page when the sign-in has gone
// (kept in this file: the layout policy counts new files in a package not yet migrated)
// ==========================================================================================
/**
 * #4966 AC2 — with the sign-in gone, the open page says so without Jeff sending anything, shows
 * nothing of the room, and refuses attach and record with a reason he can see.
 *
 * Jeff, 2026-11-13, walking: "It looks like I'm logged in ... I can't attach a transcription.
 * I can't attach a photo. No errors happen." Measured 2026-11-15: the refusals were written to
 * #send-error, which style.css hides, and nothing told the open page its sign-in had gone.
 */
{
  /* eslint-disable @typescript-eslint/no-var-requires */
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
}
