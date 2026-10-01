// @test-type: unit — POSTs images to the in-process Clearing; sips stubbed on PATH; uploads land in a temp dir.
// @card: #4417
// @owner: wren
/**
 * #4232 — images are how Jeff shows us what he sees. POST /api/upload stores
 * the picture and hands back a URL the room can show.
 * #4417 — the route was named by one test file, which wrote into the live
 * /tmp/bridge-uploads. This drives it end to end in the test's own directory.
 */
import { useInProcessClearing } from './lib/in-process-clearing'; // first: sets the temp guard key path
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const BIN = fs.mkdtempSync(path.join(os.tmpdir(), 'upload-bin-'));
// sips: write the --out file, the way the real one converts HEIC to JPEG
fs.writeFileSync(path.join(BIN, 'sips'), '#!/bin/sh\nfor a in "$@"; do if [ "$prev" = "--out" ]; then printf "JPEG" > "$a"; fi; prev="$a"; done\n', { mode: 0o755 });
const PATH_BEFORE = process.env.PATH;
process.env.PATH = `${BIN}:${process.env.PATH}`;
afterAll(() => { process.env.PATH = PATH_BEFORE; fs.rmSync(BIN, { recursive: true, force: true }); });

const gate = useInProcessClearing();
const DIR = () => process.env.CLEARING_UPLOAD_DIR as string;

async function upload(body: Buffer, type: string): Promise<{ url: string; filename: string }> {
  const res = await fetch(`${gate.base()}/api/upload`, { method: 'POST', body: new Uint8Array(body), headers: { 'Content-Type': type } });
  expect(res.status).toBe(200);
  return res.json() as Promise<{ url: string; filename: string }>;
}

describe('#4232 a picture Jeff sends is kept and shown', () => {
  test('a PNG is stored as sent and served back at the URL the room shows', async () => {
    const png = Buffer.from('\x89PNG fake image bytes');
    const r = await upload(png, 'image/png');
    expect(r.filename).toMatch(/^\d+\.png$/);
    expect(fs.readFileSync(path.join(DIR(), r.filename))).toEqual(png);
    const served = await fetch(`${gate.base()}${r.url}`);
    expect(served.status).toBe(200);
    expect(Buffer.from(await served.arrayBuffer())).toEqual(png);
  });

  test('an iPhone HEIC is converted to JPEG, and the HEIC is not left behind', async () => {
    const r = await upload(Buffer.from('heic bytes'), 'image/heic');
    expect(r.filename).toMatch(/^\d+\.jpg$/);
    expect(fs.readFileSync(path.join(DIR(), r.filename), 'utf8')).toBe('JPEG');
    expect(fs.existsSync(path.join(DIR(), r.filename.replace('.jpg', '.heic')))).toBe(false);
  });

  test('NEGATIVE PROOF: if conversion fails, the original HEIC is kept and served, never a missing file', async () => {
    fs.writeFileSync(path.join(BIN, 'sips'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
    const heic = Buffer.from('heic bytes 2');
    const r = await upload(heic, 'image/heic');
    expect(r.filename).toMatch(/^\d+\.heic$/);
    expect(fs.readFileSync(path.join(DIR(), r.filename))).toEqual(heic);
  });

  test('from outside, not signed in, an upload is refused and nothing is stored', async () => {
    const before = fs.readdirSync(DIR()).length;
    const r = await gate.visit('POST', '/api/upload');
    expect(r.status).toBe(401);
    expect(fs.readdirSync(DIR()).length).toBe(before);
  });
});
