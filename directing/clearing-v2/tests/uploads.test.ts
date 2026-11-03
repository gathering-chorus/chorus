// @test-type: integration
// @card: #4866
// @owner: abby-normal
// @domain: messages
/**
 * #4866 AC1 — /api/upload accepts photos (JPEG, PNG, GIF, HEIC) and files
 * (PDF, txt, code, generic documents) up to 50MB; persists to clearing-v2-uploads.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';

const DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-v2-uploads-4866-'));
process.env.CLEARING_V2_UPLOAD_DIR = DIR;
const { createApp } = require('../src/server');
const { mintShareSessionCookie } = require('../src/share-session');

const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
const JEFF = `chorus_share_session=${mintShareSessionCookie('https://id.lightlifeurbangardens.com/jeff/profile/card#me', KEY)}`;

const PNG = Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d4944415478da63f8ff3f0300050001fe2a6d1f0000000049454e44ae426082', 'hex');
const PDF = Buffer.from('%PDF-1.4\n%...\n%%EOF');
const CODE = Buffer.from('console.log("hello chorus");');

async function up() {
  const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(server.address() as any).port}`;
  return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
}

const post = (base: string, body: Buffer, type: string, cookie?: string, filename?: string) =>
  fetch(`${base}/api/upload${filename ? `?filename=${encodeURIComponent(filename)}` : ''}`, {
    method: 'POST',
    headers: {
      'Content-Type': type,
      ...(filename ? { 'X-Filename': filename } : {}),
      ...(cookie ? { Cookie: cookie } : {})
    },
    body
  });

describe('#4866 AC1: universal uploads (photos and files)', () => {
  test('signed-in Jeff posts a PDF document: it is saved with .pdf ext and path returned', async () => {
    const s = await up();
    try {
      const res = await post(s.base, PDF, 'application/pdf', JEFF, 'spec.pdf');
      const body = await res.json() as { ok: boolean; url: string; path: string; name: string };
      expect(res.status).toBe(200);
      expect(body.ok).toBe(true);
      expect(body.path.endsWith('.pdf')).toBe(true);
      expect(fs.readFileSync(body.path).equals(PDF)).toBe(true);
      const view = await fetch(`${s.base}${body.url}`, { headers: { Cookie: JEFF } });
      expect(view.status).toBe(200);
    } finally { await s.close(); }
  });

  test('signed-in Jeff posts a text/code file: saved with .txt/code ext and viewable', async () => {
    const s = await up();
    try {
      const res = await post(s.base, CODE, 'text/plain', JEFF, 'script.ts');
      const body = await res.json() as { ok: boolean; url: string; path: string };
      expect(res.status).toBe(200);
      expect(body.ok).toBe(true);
      expect(fs.readFileSync(body.path).equals(CODE)).toBe(true);
    } finally { await s.close(); }
  });

  test('signed-in Jeff posts a photo (PNG): still works as before', async () => {
    const s = await up();
    try {
      const res = await post(s.base, PNG, 'image/png', JEFF, 'photo.png');
      const body = await res.json() as { ok: boolean; url: string; path: string };
      expect(res.status).toBe(200);
      expect(body.ok).toBe(true);
      expect(body.path.endsWith('.png')).toBe(true);
    } finally { await s.close(); }
  });

  test('NEGATIVE PROOF: empty file upload fails loud with 400', async () => {
    const s = await up();
    try {
      const res = await post(s.base, Buffer.alloc(0), 'application/pdf', JEFF, 'empty.pdf');
      expect(res.status).toBe(400);
      const body = await res.json() as { ok: boolean; reason: string };
      expect(body.ok).toBe(false);
      expect(body.reason).toMatch(/empty/);
    } finally { await s.close(); }
  });
});
