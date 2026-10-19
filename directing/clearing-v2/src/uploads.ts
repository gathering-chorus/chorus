// @domain: messages
/* eslint-disable security/detect-non-literal-fs-filename -- every path is the upload folder (fixed at startup from CLEARING_V2_UPLOAD_DIR or ~/.chorus/clearing-v2-uploads) plus a numeric timestamp name; nothing from the request reaches a path */
// #4635 — Jeff 10-19 08:17: "i need a way to attach photos from phone", and 08:18 "if i can
// attach photos it will really simplify me showing u things i want to change". A photo posts
// to /api/upload, lands in a folder that survives reboots, and the message carries its full
// path: the room shows it as an image, and a role reading the message can open the file.
import express, { Request, Response } from 'express';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { execFile } from 'child_process';

/** The folder name the room recognises in a message; the full path is what roles open. */
export const UPLOAD_FOLDER = 'clearing-v2-uploads';
export const DEFAULT_UPLOAD_DIR = process.env.CLEARING_V2_UPLOAD_DIR || path.join(os.homedir(), '.chorus', UPLOAD_FOLDER);

const UPLOAD_NAME_RE = new RegExp(`/${UPLOAD_FOLDER}/(\\d+\\.(?:jpg|png|gif|heic))`, 'g');

/** The uploaded file names a message mentions, in order (served at /uploads/<name>). */
export function uploadNames(text: string): string[] {
  return Array.from(String(text || '').matchAll(UPLOAD_NAME_RE), (m) => m[1]);
}

function extFor(contentType: string): string {
  if (/hei[cf]/.test(contentType)) return 'heic';
  if (contentType.includes('png')) return 'png';
  if (contentType.includes('gif')) return 'gif';
  return 'jpg';
}

/** HEIC (iPhone) becomes JPEG with macOS sips; on failure the HEIC is kept. */
function toJpeg(heicPath: string, jpgPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    execFile('sips', ['-s', 'format', 'jpeg', heicPath, '--out', jpgPath], { timeout: 15000 }, (err) => resolve(!err));
  });
}

/** Saves one uploaded image and returns the name it is served under and its full path. */
export async function saveUpload(body: Buffer, contentType: string, dir: string, now = Date.now()): Promise<{ name: string; path: string }> {
  fs.mkdirSync(dir, { recursive: true });
  const ext = extFor(contentType);
  const saved = path.join(dir, `${now}.${ext}`);
  fs.writeFileSync(saved, body);
  if (ext === 'heic') {
    const jpg = path.join(dir, `${now}.jpg`);
    if (await toJpeg(saved, jpg)) {
      fs.unlinkSync(saved);
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

/** Photos are Jeff's own: posting and viewing both need a signed-in session. */
export function registerUploadRoutes(app: express.Express, opts: UploadOptions): void {
  const serve = express.static(opts.dir, { index: false });
  app.use('/uploads', (req: Request, res: Response, next) => {
    if (!opts.signedIn(req)) { res.status(401).json(opts.notSignedIn()); return; }
    serve(req, res, next);
  });
  app.post('/api/upload', express.raw({ type: () => true, limit: '40mb' }), async (req: Request, res: Response) => {
    if (opts.readOnly) { res.status(403).json({ ok: false, reason: opts.readOnlyReason }); return; }
    if (!opts.signedIn(req)) { res.status(401).json(opts.notSignedIn()); return; }
    const contentType = String(req.headers['content-type'] || '');
    if (!/^image\//.test(contentType) || !Buffer.isBuffer(req.body) || req.body.length === 0) {
      res.status(400).json({ ok: false, reason: 'not a photo: send the image itself with an image/* type' });
      return;
    }
    try {
      const saved = await saveUpload(req.body, contentType, opts.dir);
      res.json({ ok: true, url: `/uploads/${saved.name}`, path: saved.path });
    } catch (e) {
      res.status(500).json({ ok: false, reason: `the photo could not be saved: ${(e as Error).message}` });
    }
  });
}
