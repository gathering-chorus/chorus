// @domain: messages
/* eslint-disable security/detect-non-literal-fs-filename -- every path is the upload folder (fixed at startup from CLEARING_V2_UPLOAD_DIR or ~/.chorus/clearing-v2-uploads) plus a numeric timestamp name; nothing from the request reaches a path */
// #4635 — Jeff 10-19: attach photos from phone.
// #4866 — Jeff 11-02: attach photos and files (PDF, txt, code, generic documents) up to 50MB.
import express, { Request, Response } from 'express';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { execFile } from 'child_process';

/** The folder name the room recognises in a message; the full path is what roles open. */
export const UPLOAD_FOLDER = 'clearing-v2-uploads';
export const DEFAULT_UPLOAD_DIR = process.env.CLEARING_V2_UPLOAD_DIR || path.join(os.homedir(), '.chorus', UPLOAD_FOLDER);

const UPLOAD_NAME_RE = new RegExp(`/${UPLOAD_FOLDER}/(\\d+[-a-zA-Z0-9_.]*\\.[a-zA-Z0-9]{1,10})`, 'gi');

/** The uploaded file names a message mentions, in order (served at /uploads/<name>). */
export function uploadNames(text: string): string[] {
  return Array.from(String(text || '').matchAll(UPLOAD_NAME_RE), (m) => m[1]);
}

const MIME_MAP: Record<string, string> = {
  png: 'png',
  gif: 'gif',
  webp: 'webp',
  jpeg: 'jpg',
  jpg: 'jpg',
  pdf: 'pdf',
  'text/plain': 'txt',
  'text/csv': 'txt',
  markdown: 'md',
  json: 'json',
  'audio/webm': 'webm',
  'audio/mp4': 'm4a',
  'audio/m4a': 'm4a',
  'audio/aac': 'm4a',
  'audio/wav': 'wav',
  'audio/x-wav': 'wav',
  'audio/mpeg': 'mp3',
  'audio/mp3': 'mp3',
};

const BLOCKED_EXTS = new Set(['html', 'htm', 'svg', 'xhtml', 'xml', 'js', 'mjs', 'cgi', 'php']);

export function extFor(contentType: string, originalName?: string): string {
  if (originalName) {
    const ext = path.extname(originalName).replace(/^\./, '').toLowerCase();
    if (BLOCKED_EXTS.has(ext)) return 'txt';
    if (ext && /^[a-z0-9]{1,10}$/.test(ext)) return ext;
  }
  const ct = contentType.toLowerCase();
  if (/hei[cf]/.test(ct)) return 'heic';
  for (const [key, val] of Object.entries(MIME_MAP)) {
    if (ct.includes(key)) return val;
  }
  return 'bin';
}

/** HEIC (iPhone) becomes JPEG with macOS sips; on failure the HEIC is kept. */
function toJpeg(heicPath: string, jpgPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    execFile('sips', ['-s', 'format', 'jpeg', heicPath, '--out', jpgPath], { timeout: 15000 }, (err) => resolve(!err));
  });
}

/** Saves one uploaded file and returns the name it is served under and its full path. */
export async function saveUpload(
  body: Buffer,
  contentType: string,
  dir: string,
  now = Date.now(),
  originalName?: string
): Promise<{ name: string; path: string }> {
  fs.mkdirSync(dir, { recursive: true });
  const ext = extFor(contentType, originalName);
  const cleanOriginal = originalName
    ? path.basename(originalName, path.extname(originalName)).replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 32)
    : '';
  const fileBasename = cleanOriginal ? `${now}_${cleanOriginal}.${ext}` : `${now}.${ext}`;
  const saved = path.join(dir, fileBasename);
  fs.writeFileSync(saved, body);
  if (ext === 'heic') {
    const jpg = path.join(dir, cleanOriginal ? `${now}_${cleanOriginal}.jpg` : `${now}.jpg`);
    if (await toJpeg(saved, jpg)) {
      try {
        fs.unlinkSync(saved);
      } catch {
        // Ignored: cleanup failure of intermediate heic file
        void 0;
      }
      return { name: path.basename(jpg), path: jpg };
    }
  }
  return { name: path.basename(saved), path: saved };
}

export interface UploadOptions {
  dir: string;
  readOnly: boolean;
  readOnlyReason: string;
  /** Who the verified session cookie says is asking; null = not signed in. */
  signedIn: (req: Request) => string | null;
  notSignedIn: () => unknown;
}

/** Photos & files are Jeff's own: posting and viewing both need a signed-in session. */
export function registerUploadRoutes(app: express.Express, opts: UploadOptions): void {
  const serve = express.static(opts.dir, {
    index: false,
    setHeaders: (res, filePath) => {
      res.setHeader('X-Content-Type-Options', 'nosniff');
      const ext = path.extname(filePath).toLowerCase();
      // Photos display inline; all other uploads download as attachments
      if (!['.jpg', '.jpeg', '.png', '.gif', '.webp'].includes(ext)) {
        res.setHeader('Content-Disposition', `attachment; filename="${path.basename(filePath)}"`);
      }
    }
  });
  app.use('/uploads', (req: Request, res: Response, next) => {
    if (!opts.signedIn(req)) { res.status(401).json(opts.notSignedIn()); return; }
    serve(req, res, next);
  });
  app.post('/api/upload', (req: Request, res: Response, next) => {
    express.raw({ type: () => true, limit: '50mb' })(req, res, (err) => {
      if (err) {
        if (err.type === 'entity.too.large' || err.status === 413) {
          res.status(413).json({ ok: false, reason: 'file too large: upload limit is 50MB' });
          return;
        }
        res.status(400).json({ ok: false, reason: `upload error: ${err.message}` });
        return;
      }
      next();
    });
  }, async (req: Request, res: Response) => {
    if (opts.readOnly) { res.status(403).json({ ok: false, reason: opts.readOnlyReason }); return; }
    if (!opts.signedIn(req)) { res.status(401).json(opts.notSignedIn()); return; }
    if (!Buffer.isBuffer(req.body) || req.body.length === 0) {
      res.status(400).json({ ok: false, reason: 'empty file: send bytes with the upload' });
      return;
    }
    const contentType = String(req.headers['content-type'] || 'application/octet-stream');
    const filename = typeof req.query.filename === 'string'
      ? req.query.filename
      : (typeof req.headers['x-filename'] === 'string' ? req.headers['x-filename'] : undefined);
    try {
      const saved = await saveUpload(req.body, contentType, opts.dir, Date.now(), filename);
      res.json({ ok: true, url: `/uploads/${saved.name}`, path: saved.path, name: saved.name });
    } catch (e) {
      res.status(500).json({ ok: false, reason: `the file could not be saved: ${(e as Error).message}` });
    }
  });
}
