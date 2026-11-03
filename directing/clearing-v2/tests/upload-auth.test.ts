// @test-type: integration:ui
// @card: #4866
// @owner: abby-normal
// @domain: messages
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
