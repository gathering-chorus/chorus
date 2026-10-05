// Where the player looks for media, discovered from the mounted volumes (#4430).
// Jeff consolidates and renames the Bedroom volumes; a fixed list went stale
// (VideosRiley-Te, VideosTo-Wh missing; three unmounted volumes still listed).
// The rule, not a list:
//   video dirs  = every <root>/Videos* volume except VideosNew, plus any folder
//                 named Videos* or "video" directly under VideosNew or PhotosNew
//   photo sets  = every "photo sets - *" folder under <root>/VideosNew
// A volume that is unmounted or unreadable is skipped.
import fs from 'fs';
import path from 'path';

export const VOLUMES_ROOT = process.env.PLAYER_VOLUMES_ROOT || '/Volumes';

const CONTAINERS = ['VideosNew', 'PhotosNew'];

function subdirs(dir) {
  try {
    return fs.readdirSync(dir, { withFileTypes: true })
      .filter(e => !e.name.startsWith('.') && isDir(path.join(dir, e.name)))
      .map(e => e.name)
      .sort();
  } catch {
    return [];
  }
}

function isDir(p) {
  try { return fs.statSync(p).isDirectory(); } catch { return false; }
}

export function discoverVideoDirs(root = VOLUMES_ROOT) {
  const dirs = subdirs(root)
    .filter(name => name.startsWith('Videos') && !CONTAINERS.includes(name))
    .map(name => path.join(root, name));
  for (const container of CONTAINERS) {
    for (const name of subdirs(path.join(root, container))) {
      if (name.startsWith('Videos') || name === 'video') dirs.push(path.join(root, container, name));
    }
  }
  return dirs;
}

export function discoverPhotoSetParents(root = VOLUMES_ROOT) {
  return subdirs(path.join(root, 'VideosNew'))
    .filter(name => name.startsWith('photo sets - '))
    .map(name => path.join(root, 'VideosNew', name));
}

// True when filePath resolves inside one of roots. Compares whole path
// segments, so /Volumes/VideosAb is not inside /Volumes/VideosA.
export function isUnder(filePath, roots) {
  if (!filePath) return false;
  const resolved = path.resolve(filePath);
  return roots.some(r => resolved === r || resolved.startsWith(r + path.sep));
}
