// @domain: messages
/* eslint-disable security/detect-non-literal-fs-filename -- every path is the voice folder (fixed at startup from CLEARING_V2_VOICE_DIR or ~/.chorus/clearing-v2-voice) plus a numeric timestamp name; nothing from the request reaches a path */
// #4635 — Jeff 10-19 08:17: "and to record voice like we had in v1". The phone records, the
// audio posts to /api/voice, Library turns it into words with whisper-cli (as v1 does), and
// the words come back for the message box. Ported from v1 server.ts /api/voice, without a shell.
import express, { Request, Response } from 'express';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { execFile } from 'child_process';

export const DEFAULT_VOICE_DIR = process.env.CLEARING_V2_VOICE_DIR || path.join(os.homedir(), '.chorus', 'clearing-v2-voice');
export const WHISPER_MODEL = process.env.WHISPER_MODEL || '/opt/homebrew/share/whisper/ggml-base.en.bin';

function run(cmd: string, args: string[], timeout: number): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout, env: process.env, encoding: 'utf-8' }, (err, stdout) => (err ? reject(err) : resolve(String(stdout))));
  });
}

/** whisper-cli prints "[00:00.000 --> 00:02.000]  words"; keep the words only. */
export function cleanTranscript(raw: string): string {
  return raw.replace(/^\[.*?\]\s*/gm, '').replace(/\s+/g, ' ').trim();
}

function extFor(contentType: string): string {
  if (contentType.includes('mp4') || contentType.includes('m4a') || contentType.includes('aac')) return 'm4a';
  if (contentType.includes('ogg')) return 'ogg';
  if (contentType.includes('wav')) return 'wav';
  return 'webm';
}

/** Saves the recording (kept for replay), converts it to 16 kHz mono WAV, and transcribes it. */
export async function transcribe(body: Buffer, contentType: string, dir: string, now = Date.now()): Promise<string> {
  fs.mkdirSync(dir, { recursive: true });
  const audio = path.join(dir, `${now}.${extFor(contentType)}`);
  const wav = path.join(dir, `${now}.16k.wav`);
  fs.writeFileSync(audio, body);
  try {
    await run('ffmpeg', ['-i', audio, '-ar', '16000', '-ac', '1', '-y', wav], 15000);
    return cleanTranscript(await run('whisper-cli', ['-m', WHISPER_MODEL, '-f', wav, '--no-timestamps', '-t', '4'], 30000));
  } finally {
    try { fs.unlinkSync(wav); } catch { /* the wav is only a step */ }
  }
}

export interface VoiceOptions {
  dir: string;
  readOnly: boolean;
  readOnlyReason: string;
  signedIn: (req: Request) => string | null;
  notSignedIn: () => unknown;
}

export function registerVoiceRoutes(app: express.Express, opts: VoiceOptions): void {
  app.post('/api/voice', express.raw({ type: () => true, limit: '25mb' }), async (req: Request, res: Response) => {
    if (opts.readOnly) { res.status(403).json({ ok: false, reason: opts.readOnlyReason }); return; }
    if (!opts.signedIn(req)) { res.status(401).json(opts.notSignedIn()); return; }
    if (!Buffer.isBuffer(req.body) || req.body.length === 0) {
      res.status(400).json({ ok: false, reason: 'no audio: the recording was empty' });
      return;
    }
    try {
      const text = await transcribe(req.body, String(req.headers['content-type'] || ''), opts.dir);
      if (!text) { res.status(422).json({ ok: false, reason: 'no words heard in the recording' }); return; }
      res.json({ ok: true, transcript: text });
    } catch (e) {
      res.status(500).json({ ok: false, reason: `transcription failed: ${(e as Error).message.split('\n')[0]}` });
    }
  });
}
