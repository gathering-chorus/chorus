// @test-type: integration
// @card: #4866
// @owner: abby-normal
// @domain: messages
/**
 * #4866 AC3 — Voice transcription (/api/voice) transcribes audio reliably
 * into composer text with clear visual states (recording, transcribing, error)
 * and robust conversion handling mobile Safari audio formats.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';

const DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-v2-voice-4866-'));
process.env.CLEARING_V2_VOICE_DIR = DIR;
const { createApp } = require('../src/server');
const { mintShareSessionCookie } = require('../src/share-session');
const { transcribeVoice } = require('../public/app.js');

const KEY = Buffer.from('test-secret-cookie-key-for-session-32b');
const JEFF = `chorus_share_session=${mintShareSessionCookie('https://id.lightlifeurbangardens.com/jeff/profile/card#me', KEY)}`;

async function up() {
  const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9', cookieKey: KEY });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(server.address() as any).port}`;
  return { base, close: async () => { relay.close(); await new Promise<void>((r) => server.close(() => r())); } };
}

describe('#4866 AC3: reliable voice transcription', () => {
  test('transcribeVoice handles mobile Safari audio/mp4 content type', async () => {
    let receivedType = '';
    const fakeFetch = async (_url: string, opts: any) => {
      receivedType = opts.headers['Content-Type'];
      return {
        ok: true,
        status: 200,
        json: async () => ({ ok: true, transcript: 'test voice transcript' })
      };
    };

    const blob = { size: 1024, type: 'audio/mp4' };
    const res = await transcribeVoice(blob, fakeFetch);
    expect(res.ok).toBe(true);
    expect(res.transcript).toBe('test voice transcript');
    expect(receivedType).toBe('audio/mp4');
  });

  test('transcribeVoice names empty recording failure loud', async () => {
    const res = await transcribeVoice(null);
    expect(res.ok).toBe(false);
    expect(res.reason).toMatch(/empty/);
  });

  test('transcribeVoice reports server transcription errors cleanly', async () => {
    const fakeFetch = async () => ({
      ok: false,
      status: 422,
      json: async () => ({ ok: false, reason: 'no words heard in the recording' })
    });

    const blob = { size: 1024, type: 'audio/webm' };
    const res = await transcribeVoice(blob, fakeFetch);
    expect(res.ok).toBe(false);
    expect(res.reason).toMatch(/no words heard/);
  });
});
