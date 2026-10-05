// Re-point playlist videos whose files moved to another volume (#4430).
// Jeff consolidates volumes by moving model folders; the file name stays the
// same, so a missing entry is found again by its file name. Only a name that
// matches exactly one file is repointed; ambiguous and gone entries are left
// as they are and counted.
import fs from 'fs';
import path from 'path';

export const videoUrl = p => `/api/proxy/video?path=${encodeURIComponent(p)}`;

export function filePathOf(url) {
  const i = url.indexOf('path=');
  if (i < 0) return url;
  try { return decodeURIComponent(url.slice(i + 5)); } catch { return null; }
}

export function indexFilesByName(dirs) {
  const index = new Map();
  const walk = dir => {
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (e.name.startsWith('.')) continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.isFile()) {
        const list = index.get(e.name);
        if (list) list.push(p); else index.set(e.name, [p]);
      }
    }
  };
  for (const d of dirs) walk(d);
  return index;
}

export function repairVideoPaths(data, index, exists = fs.existsSync) {
  const counts = { ok: 0, repaired: 0, ambiguous: 0, missing: 0 };
  for (const item of data.videos) {
    const file = filePathOf(item.path);
    if (file && exists(file)) { counts.ok++; continue; }
    const matches = file ? index.get(path.basename(file)) || [] : [];
    if (matches.length === 1) {
      item.path = videoUrl(matches[0]);
      counts.repaired++;
    } else if (matches.length > 1) {
      counts.ambiguous++;
    } else {
      counts.missing++;
    }
  }
  return counts;
}
