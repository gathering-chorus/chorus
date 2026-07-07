// @test-type: integration — touches a real tmpdir (mkdtemp), not unit; no live services.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { loadPlaylists, savePlaylists, addItem, removeItem } from '../lib/playlist-store.js';

function tmpFile() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sp-playlist-'));
  return path.join(dir, 'playlists.json');
}

test('load missing file returns empty structure', () => {
  const file = tmpFile();
  assert.deepEqual(loadPlaylists(file), { photos: [], videos: [] });
});

test('save/load roundtrip', () => {
  const file = tmpFile();
  const data = { photos: [{ path: '/a', name: 'a' }], videos: [{ path: '/v', name: 'v' }] };
  savePlaylists(file, data);
  assert.deepEqual(loadPlaylists(file), data);
});

test('save is atomic — no temp file left behind, valid JSON on disk', () => {
  const file = tmpFile();
  savePlaylists(file, { photos: [], videos: [] });
  const leftovers = fs.readdirSync(path.dirname(file)).filter(f => f.includes('.tmp'));
  assert.deepEqual(leftovers, []);
  JSON.parse(fs.readFileSync(file, 'utf8'));
});

test('playlists well past the old 100KB express limit roundtrip intact', () => {
  const file = tmpFile();
  const data = { photos: [], videos: [] };
  for (let i = 0; i < 2000; i++) {
    data.videos.push({ path: `/Volumes/VideosMulti/model-${i}/some-long-video-file-name-${i}.mp4`, name: `some-long-video-file-name-${i}` });
  }
  savePlaylists(file, data);
  assert.ok(fs.statSync(file).size > 150 * 1024, 'fixture should exceed 150KB');
  assert.equal(loadPlaylists(file).videos.length, 2000);
});

test('addItem appends and dedups by path', () => {
  const data = { photos: [], videos: [] };
  addItem(data, 'videos', { path: '/v1', name: 'v1' });
  addItem(data, 'videos', { path: '/v1', name: 'v1 again' });
  assert.equal(data.videos.length, 1);
  assert.equal(data.videos[0].name, 'v1');
});

test('removeItem removes by path, ignores missing', () => {
  const data = { photos: [{ path: '/p1', name: 'p1' }], videos: [] };
  removeItem(data, 'photos', '/p1');
  removeItem(data, 'photos', '/nope');
  assert.equal(data.photos.length, 0);
});

test('addItem validates type and path', () => {
  assert.throws(() => addItem({ photos: [], videos: [] }, 'bogus', { path: '/x' }));
  assert.throws(() => addItem({ photos: [], videos: [] }, 'photos', { name: 'no path' }));
});

test('load tolerates corrupt file', () => {
  const file = tmpFile();
  fs.writeFileSync(file, '{not json');
  assert.deepEqual(loadPlaylists(file), { photos: [], videos: [] });
});
