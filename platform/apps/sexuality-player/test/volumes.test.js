// @test-type: unit — real dirs in a temp folder stand in for /Volumes; no services.
// #4430: the player finds its folders from the mounted volumes, not a fixed list.
import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { discoverVideoDirs, discoverPhotoSetParents, isUnder } from '../lib/volumes.js';

let root;
const mk = (...parts) => fs.mkdirSync(path.join(root, ...parts), { recursive: true });
const names = dirs => dirs.map(d => path.relative(root, d));

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), 'player-4430-'));
  mk('Macintosh HD');
  mk('VideosAbella-Alexa', 'abella-danger');
  mk('VideosMulti', 'VideosKata-Kev');
  mk('VideosNew', 'photo sets - 🟣', 'A');
  mk('VideosNew', 'photo sets - 🔴');
  mk('VideosNew', 'Gathering');
  mk('VideosNew', 'Models');
  mk('VideosNew', 'video');
  mk('VideosNew', 'VideosTb-Uma');
  mk('PhotosNew', 'VideosKenn-Kenz');
  mk('PhotosNew', 'thumbnails');
});

afterEach(() => {
  for (const d of fs.readdirSync(root)) fs.chmodSync(path.join(root, d), 0o755);
  fs.rmSync(root, { recursive: true, force: true });
});

describe('discoverVideoDirs', () => {
  it('finds a new Videos* volume with no code change', () => {
    mk('VideosRiley-Te', 'riley-reid');
    assert.ok(names(discoverVideoDirs(root)).includes('VideosRiley-Te'));
  });

  it('finds Videos* and video folders inside VideosNew and PhotosNew, and nothing else there', () => {
    assert.deepEqual(names(discoverVideoDirs(root)), [
      'VideosAbella-Alexa', 'VideosMulti',
      'VideosNew/VideosTb-Uma', 'VideosNew/video',
      'PhotosNew/VideosKenn-Kenz',
    ]);
  });

  it('drops a volume once it is unmounted', () => {
    fs.rmSync(path.join(root, 'VideosAbella-Alexa'), { recursive: true });
    assert.ok(!names(discoverVideoDirs(root)).includes('VideosAbella-Alexa'));
  });

  it('returns nothing, without throwing, when the volumes root is missing', () => {
    assert.deepEqual(discoverVideoDirs(path.join(root, 'nope')), []);
  });
});

describe('discoverPhotoSetParents', () => {
  it('finds every "photo sets - *" folder under VideosNew', () => {
    assert.deepEqual(names(discoverPhotoSetParents(root)),
      ['VideosNew/photo sets - 🔴', 'VideosNew/photo sets - 🟣']);
  });
});

describe('isUnder (the proxy allow-list)', () => {
  it('allows a video on a newly discovered volume', () => {
    mk('VideosTo-Wh', 'tobee');
    const file = path.join(root, 'VideosTo-Wh', 'tobee', 'clip.mp4');
    assert.equal(isUnder(file, discoverVideoDirs(root)), true);
  });

  it('refuses a path outside the discovered set', () => {
    assert.equal(isUnder(path.join(root, 'VideosNew', 'Gathering', 'x.mp4'), discoverVideoDirs(root)), false);
    assert.equal(isUnder('/etc/passwd', discoverVideoDirs(root)), false);
  });

  it('refuses ../ that climbs out of an allowed folder', () => {
    const sneaky = path.join(root, 'VideosAbella-Alexa', '..', 'VideosNew', 'Gathering', 'x.mp4');
    assert.equal(isUnder(sneaky, discoverVideoDirs(root)), false);
  });

  it('does not treat a longer sibling name as inside (VideosAbella-Alexa2)', () => {
    assert.equal(isUnder(path.join(root, 'VideosAbella-Alexa2', 'x.mp4'), discoverVideoDirs(root)), false);
  });

  it('refuses an empty path', () => {
    assert.equal(isUnder('', discoverVideoDirs(root)), false);
  });
});
