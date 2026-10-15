// @test-type: unit — real files in a temp folder; no services.
// #4430: playlist videos that moved volumes are found again by file name.
import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { indexFilesByName, repairVideoPaths, videoUrl, filePathOf } from '../lib/playlist-repair.js';

let root;
const put = (...parts) => {
  const p = path.join(root, ...parts);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, 'v');
  return p;
};

beforeEach(() => { root = fs.mkdtempSync(path.join(os.tmpdir(), 'playlist-4430-')); });
afterEach(() => fs.rmSync(root, { recursive: true, force: true }));

describe('repairVideoPaths', () => {
  it('re-points a video that moved to another volume, keeping the rest of the item', () => {
    const moved = put('VideosMulti', 'VideosWi-Zz', 'zara-jade', 'zara-jade-solo.mp4');
    const data = { photos: [], videos: [{
      path: videoUrl(path.join(root, 'VideosUma-Zaa', 'zara-jade', 'zara-jade-solo.mp4')),
      name: 'zara-jade-solo.mp4', model: 'zara-jade',
    }] };

    const counts = repairVideoPaths(data, indexFilesByName([path.join(root, 'VideosMulti')]));

    assert.deepEqual(counts, { ok: 0, repaired: 1, ambiguous: 0, missing: 0 });
    assert.equal(filePathOf(data.videos[0].path), moved);
    assert.equal(data.videos[0].model, 'zara-jade');
  });

  it('leaves a video that still exists alone', () => {
    const here = put('VideosA', 'm', 'a.mp4');
    const data = { photos: [], videos: [{ path: videoUrl(here) }] };
    const before = data.videos[0].path;
    assert.equal(repairVideoPaths(data, indexFilesByName([root])).ok, 1);
    assert.equal(data.videos[0].path, before);
  });

  it('does not guess when two files share the name', () => {
    put('VideosA', 'm1', 'same.mp4');
    put('VideosB', 'm2', 'same.mp4');
    const gone = videoUrl(path.join(root, 'VideosOld', 'm', 'same.mp4'));
    const data = { photos: [], videos: [{ path: gone }] };
    assert.equal(repairVideoPaths(data, indexFilesByName([root])).ambiguous, 1);
    assert.equal(data.videos[0].path, gone);
  });

  it('counts a video whose file is gone everywhere, and leaves it', () => {
    const gone = videoUrl(path.join(root, 'VideosOld', 'm', 'deleted.mp4'));
    const data = { photos: [], videos: [{ path: gone }] };
    assert.equal(repairVideoPaths(data, indexFilesByName([root])).missing, 1);
    assert.equal(data.videos[0].path, gone);
  });

  it('survives a malformed path', () => {
    const data = { photos: [], videos: [{ path: '/api/proxy/video?path=%E0%A4%A' }] };
    assert.equal(repairVideoPaths(data, new Map()).missing, 1);
  });
});
