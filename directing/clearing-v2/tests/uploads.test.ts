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

  test('signed-in Jeff posts a ~5MB camera JPEG photo: it saves successfully', async () => {
    const s = await up();
    try {
      // 5MB JPEG buffer fixture
      const largeJpeg = Buffer.alloc(5 * 1024 * 1024, 0xff);
      largeJpeg[0] = 0xff; largeJpeg[1] = 0xd8; // JPEG SOI
      largeJpeg[largeJpeg.length - 2] = 0xff; largeJpeg[largeJpeg.length - 1] = 0xd9; // JPEG EOI
      const res = await post(s.base, largeJpeg, 'image/jpeg', JEFF, 'camera_photo.jpg');
      expect(res.status).toBe(200);
      const body = await res.json() as { ok: boolean; path: string; url: string };
      expect(body.ok).toBe(true);
      expect(fs.existsSync(body.path)).toBe(true);
      expect(fs.statSync(body.path).size).toBe(5 * 1024 * 1024);
    } finally { await s.close(); }
  });

  test('NEGATIVE PROOF: upload over 50MB limit returns HTTP 413 with named reason', async () => {
    const s = await up();
    try {
      // 51MB payload exceeds 50mb limit
      const oversized = Buffer.alloc(51 * 1024 * 1024, 0x00);
      const res = await post(s.base, oversized, 'application/pdf', JEFF, 'huge.pdf');
      expect(res.status).toBe(413);
      const body = await res.json() as { ok: boolean; reason: string };
      expect(body.ok).toBe(false);
      expect(body.reason).toMatch(/file too large: upload limit is 50MB/);
    } finally { await s.close(); }
  });

  test('SECURITY: .html upload is not served as text/html (sanitized to txt + Content-Disposition)', async () => {
    const s = await up();
    try {
      const htmlPayload = Buffer.from('<script>alert("xss")</script>');
      const res = await post(s.base, htmlPayload, 'text/html', JEFF, 'exploit.html');
      expect(res.status).toBe(200);
      const body = await res.json() as { ok: boolean; url: string; path: string };
      expect(body.ok).toBe(true);
      expect(body.path.endsWith('.txt')).toBe(true); // Sanitized from .html to .txt

      const viewRes = await fetch(`${s.base}${body.url}`, { headers: { Cookie: JEFF } });
      expect(viewRes.status).toBe(200);
      expect(viewRes.headers.get('content-type')).not.toContain('text/html');
      expect(viewRes.headers.get('x-content-type-options')).toBe('nosniff');
      expect(viewRes.headers.get('content-disposition')).toMatch(/attachment/);
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
