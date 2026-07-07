import fs from 'fs';
import path from 'path';

const EMPTY = () => ({ photos: [], videos: [] });

export function loadPlaylists(file) {
  try {
    const data = JSON.parse(fs.readFileSync(file, 'utf8'));
    return {
      photos: Array.isArray(data.photos) ? data.photos : [],
      videos: Array.isArray(data.videos) ? data.videos : [],
    };
  } catch {
    return EMPTY();
  }
}

// Atomic: write to a temp file in the same directory, then rename over the target.
// A crash mid-write can never leave a truncated playlists.json.
export function savePlaylists(file, data) {
  const tmp = path.join(path.dirname(file), `.${path.basename(file)}.tmp`);
  fs.writeFileSync(tmp, JSON.stringify(data, null, 2));
  fs.renameSync(tmp, file);
}

export function addItem(data, type, item) {
  if (type !== 'photos' && type !== 'videos') throw new Error(`bad type: ${type}`);
  if (!item || typeof item.path !== 'string' || !item.path) throw new Error('item.path required');
  if (!data[type].some(p => p.path === item.path)) data[type].push(item);
  return data;
}

export function removeItem(data, type, itemPath) {
  if (type !== 'photos' && type !== 'videos') throw new Error(`bad type: ${type}`);
  const idx = data[type].findIndex(p => p.path === itemPath);
  if (idx >= 0) data[type].splice(idx, 1);
  return data;
}
