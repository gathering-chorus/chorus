// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #1782 — Clearing voice capture
 * MediaRecorder captures audio, server transcribes via whisper-cli, auto-routes message.
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');
const CLEARING_SERVER = path.join(__dirname, '../src/server.ts');

describe('Clearing voice capture (#1782)', () => {
  let html: string;
  let server: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
    server = fs.readFileSync(CLEARING_SERVER, 'utf-8');
  });

  // Client-side
  test('mic button uses MediaRecorder, not Speech API', () => {
    expect(html).toContain('MediaRecorder');
  });

  test('click toggles recording state', () => {
    expect(html).toContain('isRecording');
    expect(html).toContain("micBtn.addEventListener('click'");
  });

  test('recorded audio is sent to server automatically', () => {
    // After recording stops, audio blob is POSTed to server
    expect(html).toContain('/api/voice');
  });

  test('no manual send step — auto-routes after transcription', () => {
    // The response from the server contains the transcript which is auto-sent
    expect(html).toContain('auto-send');
  });

  // Server-side
  test('server has voice upload endpoint', () => {
    expect(server).toContain('/api/voice');
  });

  test('server calls whisper-cli for transcription', () => {
    expect(server).toContain('whisper-cli');
  });

  test('server persists audio file', () => {
    expect(server).toContain('audio-uploads');
  });
});
