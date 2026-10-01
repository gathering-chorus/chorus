// @test-type: unit — POSTs audio to the in-process Clearing with ffmpeg and whisper-cli stubbed on PATH; audio lands in a temp dir.
/**
 * #3857 voice capture, server half. Jeff taps the mic, the page uploads the
 * recording, the Clearing keeps the audio and returns a transcript that the
 * page sends as his message. #4417 — this used to match "whisper-cli" and
 * "audio-uploads" in server.ts; it now posts audio and reads what comes back.
 * The page half runs in a browser (proving/flows/clearing-page-3857.spec.cjs).
 */
import * as fs from 'fs';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import type { AddressInfo } from 'net';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'voice-3857-'));
const BIN = path.join(TMP, 'bin');
fs.mkdirSync(BIN);
// ffmpeg: the last argument is the wav it should write. whisper-cli: prints a
// timestamped line the way the real one does, so the server's cleanup runs.
fs.writeFileSync(path.join(BIN, 'ffmpeg'), '#!/bin/sh\nfor a in "$@"; do last="$a"; done\n: > "$last"\n', { mode: 0o755 });
fs.writeFileSync(path.join(BIN, 'whisper-cli'), '#!/bin/sh\necho "[00:00:00.000 --> 00:00:02.000]   wren what is on the board"\n', { mode: 0o755 });

let srv: { server: http.Server; io: { close: () => void } };
let base = '';
const PATH_BEFORE = process.env.PATH;

beforeAll(async () => {
  process.env.PATH = `${BIN}:${process.env.PATH}`;
  process.env.PULSE_URL = 'http://127.0.0.1:1';
  process.env.CHORUS_ROOT = TMP;
  process.env.CLEARING_SCAN_DIR = TMP;
  process.env.CLEARING_PROJECTS_DIR = TMP;
  process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
  srv = require('../src/server');
  await new Promise<void>((r) => srv.server.listen(0, '127.0.0.1', () => r()));
  base = `http://127.0.0.1:${(srv.server.address() as AddressInfo).port}`;
});

afterAll(async () => {
  process.env.PATH = PATH_BEFORE;
  srv?.io.close();
  if (srv) await new Promise<void>((r) => srv.server.close(() => r()));
  fs.rmSync(TMP, { recursive: true, force: true });
});

describe('#3857 Jeff\'s voice note is kept and transcribed', () => {
  test('audio in → transcript out, and the recording is kept where the page can play it back', async () => {
    const audio = Buffer.from('fake-webm-bytes');
    const res = await fetch(`${base}/api/voice`, { method: 'POST', body: audio, headers: { 'Content-Type': 'audio/webm' } });
    const body = await res.json() as { transcript?: string; audioFile?: string; error?: string };
    expect(body.error).toBeUndefined();
    expect(body.transcript).toBe('wren what is on the board');
    const stored = path.join(process.env.CLEARING_AUDIO_DIR as string, path.basename(body.audioFile || ''));
    expect(fs.readFileSync(stored)).toEqual(audio);
    const served = await fetch(`${base}${body.audioFile}`);
    expect(served.status).toBe(200);
  });

  test('NEGATIVE PROOF: an empty recording is refused, and nothing is stored', async () => {
    const before = fs.existsSync(process.env.CLEARING_AUDIO_DIR as string) ? fs.readdirSync(process.env.CLEARING_AUDIO_DIR as string).length : 0;
    const res = await fetch(`${base}/api/voice`, { method: 'POST', body: Buffer.alloc(0) });
    expect(await res.json()).toEqual({ error: 'empty audio' });
    const after = fs.existsSync(process.env.CLEARING_AUDIO_DIR as string) ? fs.readdirSync(process.env.CLEARING_AUDIO_DIR as string).length : 0;
    expect(after).toBe(before);
  });

  test('NEGATIVE PROOF: a failed transcription says so, never an empty transcript', async () => {
    fs.writeFileSync(path.join(BIN, 'whisper-cli'), '#!/bin/sh\nexit 3\n', { mode: 0o755 });
    const res = await fetch(`${base}/api/voice`, { method: 'POST', body: Buffer.from('x') });
    const body = await res.json() as { transcript?: string; error?: string };
    expect(body.transcript).toBeUndefined();
    expect(body.error).toMatch(/^Transcription failed/);
  });
});
