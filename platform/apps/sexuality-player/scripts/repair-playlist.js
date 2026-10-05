#!/usr/bin/env node
// Re-point playlist videos whose files moved volumes (#4430).
//   node scripts/repair-playlist.js            dry run: print counts, write nothing
//   node scripts/repair-playlist.js --apply    copy playlists.json to playlists.json.bak-<time>, then save
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { discoverVideoDirs } from '../lib/volumes.js';
import { loadPlaylists, savePlaylists } from '../lib/playlist-store.js';
import { indexFilesByName, repairVideoPaths } from '../lib/playlist-repair.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const file = process.env.PLAYLIST_FILE || path.join(here, '..', 'playlists.json');
const apply = process.argv.includes('--apply');

const data = loadPlaylists(file);
const index = indexFilesByName(discoverVideoDirs());
const counts = repairVideoPaths(data, index);
console.log(`${data.videos.length} playlist videos:`, counts);

if (!apply) {
  console.log('dry run: nothing written (pass --apply)');
} else if (counts.repaired === 0) {
  console.log('nothing to repair');
} else {
  const backup = `${file}.bak-${new Date().toISOString().replace(/[:.]/g, '-')}`;
  fs.copyFileSync(file, backup);
  savePlaylists(file, data);
  console.log(`saved; previous copy at ${backup}`);
}
