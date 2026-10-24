// @test-type: integration — the real app on a local port with a test cookie key and its own upload folder; no live relay.
// @card: #4635
// @owner: wren
// @domain: messages
/**
 * #4635 — Jeff 10-19 08:17: "i need a way to attach photos from phone", 08:18: "if i can
 * attach photos it will really simplify me showing u things i want to change". A signed-in
 * Jeff posts a photo, the message carries its path, the room shows it, and nobody who is not
 * signed in can post or see one.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import { execFileSync } from 'child_process';

const DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-v2-uploads-'));
process.env.CLEARING_V2_UPLOAD_DIR = DIR;
const { createApp } = require('../src/server');
const { mintShareSessionCookie } = require('../src/share-session');
const { roomPhotos } = require('../src/views');
const { uploadPhoto, photoNames } = require('../public/app.js');

const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
const JEFF = `chorus_share_session=${mintShareSessionCookie('https://id.lightlifeurbangardens.com/jeff/profile/card#me', KEY)}`;
// 1x1 PNG
const PNG = Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d4944415478da63f8ff3f0300050001fe2a6d1f0000000049454e44ae426082', 'hex');

async function up(readOnly = false) {
  const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY, readOnly });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(server.address() as any).port}`;
  return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
}
const post = (base: string, body: Buffer, type: string, cookie?: string) =>
  fetch(`${base}/api/upload`, { method: 'POST', headers: { 'Content-Type': type, ...(cookie ? { Cookie: cookie } : {}) }, body });

describe('#4635 attach a photo', () => {
  test('signed-in Jeff posts a photo: it is saved, its path comes back, and he can view it', async () => {
    const s = await up();
    try {
      const res = await post(s.base, PNG, 'image/png', JEFF);
      const body = await res.json() as { ok: boolean; url: string; path: string };
      expect(res.status).toBe(200);
      expect(body.ok).toBe(true);
      expect(body.path.startsWith(DIR)).toBe(true);
      expect(fs.readFileSync(body.path).equals(PNG)).toBe(true);
      const view = await fetch(`${s.base}${body.url}`, { headers: { Cookie: JEFF } });
      expect(view.status).toBe(200);
    } finally { await s.close(); }
  });

  test('NEGATIVE PROOF: not signed in cannot post or view a photo', async () => {
    const s = await up();
    try {
      const res = await post(s.base, PNG, 'image/png');
      expect(res.status).toBe(401);
      const saved = await (await post(s.base, PNG, 'image/png', JEFF)).json() as { url: string };
      expect((await fetch(`${s.base}${saved.url}`)).status).toBe(401);
    } finally { await s.close(); }
  });

  test('a read-only demo refuses, and a non-image is refused by name', async () => {
    const ro = await up(true);
    try { expect((await post(ro.base, PNG, 'image/png', JEFF)).status).toBe(403); } finally { await ro.close(); }
    const s = await up();
    try {
      const res = await post(s.base, Buffer.from('hello'), 'text/plain', JEFF);
      expect(res.status).toBe(400);
      expect(((await res.json()) as { reason: string }).reason).toMatch(/not a photo/);
    } finally { await s.close(); }
  });

  test('an iPhone HEIC photo is stored as JPEG', async () => {
    const png = path.join(DIR, 'src.png');
    const heic = path.join(DIR, 'src.heic');
    fs.writeFileSync(png, PNG);
    execFileSync('sips', ['-s', 'format', 'heic', png, '--out', heic]);
    const s = await up();
    try {
      const body = await (await post(s.base, fs.readFileSync(heic), 'image/heic', JEFF)).json() as { path: string };
      expect(body.path.endsWith('.jpg')).toBe(true);
      expect(fs.existsSync(body.path)).toBe(true);
    } finally { await s.close(); }
  });

  test('a message that names a photo shows it in the room; one that does not shows none', () => {
    const p = `${DIR}/1760890000000.jpg`.replace(/.*\/(clearing-v2-uploads)[^/]*\//, '/Users/jeff/.chorus/$1/');
    expect(photoNames(`📷 ${p} the send button is here`)).toEqual(['1760890000000.jpg']);
    expect(roomPhotos(`📷 ${p} look`)).toContain('<img src="/uploads/1760890000000.jpg"');
    expect(roomPhotos('no photo here')).toBe('');
  });

  test('the composer names an upload failure instead of saying nothing', async () => {
    const failed = await uploadPhoto({ name: 'a.png', type: 'image/png' }, async () => ({ ok: false, status: 401, json: async () => ({ detail: 'Not signed in here.' }) }));
    expect(failed).toEqual({ ok: false, reason: 'Not signed in here.' });
    const heic = await uploadPhoto({ name: 'IMG_1.HEIC', type: '' }, async (_u: string, o: any) => ({ ok: true, status: 200, json: async () => ({ ok: true, path: o.headers['Content-Type'] }) }));
    expect(heic).toEqual({ ok: true, path: 'image/heic' });
  });
});

describe('#4635 record voice', () => {
  const { cleanTranscript, transcribe } = require('../src/voice');
  const { transcribeVoice } = require('../public/app.js');

  test('whisper output loses its timestamps and keeps the words', () => {
    expect(cleanTranscript('[00:00:00.000 --> 00:00:02.000]   the send button\n[00:00:02.000 --> 00:00:03.000]  is too low')).toBe('the send button is too low');
  });

  test('a real recording becomes words on Library (ffmpeg + whisper-cli)', async () => {
    const aiff = path.join(DIR, 'say.aiff');
    execFileSync('say', ['-v', 'Samantha', '-o', aiff, 'the send button is too low']);
    const text = await transcribe(fs.readFileSync(aiff), 'audio/aiff', DIR);
    expect(text.toLowerCase()).toContain('send button');
  }, 60000);

  test('NEGATIVE PROOF: not signed in cannot use voice; an empty recording is refused by name', async () => {
    const s = await up();
    try {
      const anon = await fetch(`${s.base}/api/voice`, { method: 'POST', headers: { 'Content-Type': 'audio/webm' }, body: Buffer.from('x') });
      expect(anon.status).toBe(401);
      const empty = await fetch(`${s.base}/api/voice`, { method: 'POST', headers: { 'Content-Type': 'audio/webm', Cookie: JEFF }, body: Buffer.alloc(0) });
      expect(empty.status).toBe(400);
    } finally { await s.close(); }
  });

  test('the composer names a voice failure instead of saying nothing', async () => {
    const out = await transcribeVoice({ size: 5, type: 'audio/mp4' }, async () => ({ ok: false, status: 500, json: async () => ({ reason: 'transcription failed: whisper-cli missing' }) }));
    expect(out).toEqual({ ok: false, reason: 'transcription failed: whisper-cli missing' });
    expect(await transcribeVoice({ size: 0 })).toEqual({ ok: false, reason: 'the recording was empty' });
  });
});

describe('#4635 a new version reaches the phone at once', () => {
  test('every page asks for the script and stylesheet by version, so a 4-hour cache cannot serve the old one', () => {
    const { renderRoomsPage, ASSET_VERSION } = require('../src/views');
    const html = renderRoomsPage('team', { room: 'team', channels: ['team'], directRoles: [], messages: [] }, []);
    expect(ASSET_VERSION).toMatch(/^\d{10,}$/);
    for (const f of ['/style.css', '/app.js', '/room-key.js']) expect(html).toContain(`${f}?v=${ASSET_VERSION}`);
    expect(html).not.toMatch(/(href|src)="\/(style\.css|app\.js)"/);
  });
});

describe('#4635 several photos at once', () => {
  test('Jeff picks several photos and every one lands in the message box; a failed one is counted', async () => {
    const { watchPhotoButton } = require('../public/app.js');
    let onChange: () => Promise<void> = async () => {};
    const picker = { files: [{ name: 'a.jpg', type: 'image/jpeg' }, { name: 'b.jpg', type: 'image/jpeg' }, { name: 'c.jpg', type: 'image/jpeg' }], value: 'x', addEventListener: (_e: string, f: () => Promise<void>) => { onChange = f; } };
    const input = { value: '@silas', focus: () => {} };
    const status = { textContent: '' };
    (global as any).document = { getElementById: (id: string) => ({ 'composer-photo': picker, 'composer-input': input, 'send-error': status } as any)[id] || null };
    let n = 0;
    (global as any).fetch = async () => { n += 1; return n === 2
      ? { ok: false, status: 500, json: async () => ({ reason: 'disk full' }) }
      : { ok: true, status: 200, json: async () => ({ ok: true, path: `/x/clearing-v2-uploads/${n}.jpg` }) }; };
    watchPhotoButton();
    await onChange();
    expect(input.value).toBe('@silas 📷 /x/clearing-v2-uploads/1.jpg 📷 /x/clearing-v2-uploads/3.jpg ');
    expect(status.textContent).toBe('1 of 3 photos not attached: disk full');
    delete (global as any).document; delete (global as any).fetch;
  });

  test('the photo picker allows several', () => {
    const { renderRoomsPage } = require('../src/views');
    expect(renderRoomsPage('team', { room: 'team', channels: ['team'], directRoles: [], messages: [] }, [])).toContain('accept="image/*" multiple');
  });
});
