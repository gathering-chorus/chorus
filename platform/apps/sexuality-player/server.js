import express from 'express';
import cors from 'cors';
import fs from 'fs';
import path from 'path';
import { execFile } from 'child_process';
import { fileURLToPath } from 'url';
import { parseSetFolder, getTagFromParent } from './lib/parser.js';
import { loadPlaylists, savePlaylists, addItem, removeItem } from './lib/playlist-store.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const app = express();
const PORT = 8090;

// Photo set parent folders on /Volumes/VideosNew
const PHOTO_SET_PARENTS = [
  '/Volumes/VideosNew/photo sets - 💄',
  '/Volumes/VideosNew/photo sets - 🔴',
  '/Volumes/VideosNew/photo sets - 🟠',
  '/Volumes/VideosNew/photo sets - 🟢',
  '/Volumes/VideosNew/photo sets - 🟣',
];

// Video source directories — all mounted video volumes
const VIDEO_DIRS = [
  '/Volumes/VideosAbella-Alexa',
  '/Volumes/VideosAlexa-Amb',
  '/Volumes/VideosAme-Aria',
  '/Volumes/VideosAria-Bianca',
  '/Volumes/VideosBianka-Chan',
  '/Volumes/VideosChan-Coco',
  '/Volumes/VideosCoco-Eliza',
  '/Volumes/VideosEliza-Erica',
  '/Volumes/VideosErica-Haley',
  '/Volumes/VideosHaley-Hime',
  '/Volumes/VideosHime-Jeff',
  '/Volumes/VideosJeff-Kata',
  '/Volumes/VideosKey-Lea',
  '/Volumes/VideosLeb-Luci',
  '/Volumes/VideosLucj-Maria',
  '/Volumes/VideosMaria-Mega',
  '/Volumes/VideosMega-Mia',
  '/Volumes/VideosMia-Nat',
  '/Volumes/VideosMulti',
  '/Volumes/VideosNew/video',
  '/Volumes/VideosNia-Rilex',
  '/Volumes/VideosRilez-Ta',
  '/Volumes/VideosTb-Uma',
  '/Volumes/VideosUma-Zaa',
];

// Media server base URL (images-api-video on localhost)
const MEDIA_SERVER = 'http://localhost:8082';

// Playlist persistence file
const PLAYLIST_FILE = path.join(__dirname, 'playlists.json');

app.use(cors());
// Playlist blobs can exceed express.json's 100KB default — that default silently
// 413'd saves once the playlist grew past ~190 items (#3624).
app.use(express.json({ limit: '20mb' }));
app.use(express.static(path.join(__dirname, 'public')));

// ── Helpers ──


async function countFiles(dirPath) {
  try {
    const entries = await fs.promises.readdir(dirPath);
    const imageExts = ['.jpg', '.jpeg', '.png', '.gif', '.webp'];
    return entries.filter(e => imageExts.includes(path.extname(e).toLowerCase())).length;
  } catch {
    return 0;
  }
}

async function scanPhotoSets() {
  const sets = [];

  for (const parent of PHOTO_SET_PARENTS) {
    const tag = getTagFromParent(parent);
    let alphaEntries;
    try {
      alphaEntries = await fs.promises.readdir(parent, { withFileTypes: true });
    } catch { continue; }

    // Process alpha folders in parallel
    const alphaPromises = alphaEntries
      .filter(e => e.isDirectory())
      .map(async (alphaEntry) => {
        const alphaPath = path.join(parent, alphaEntry.name);
        let modelEntries;
        try {
          modelEntries = await fs.promises.readdir(alphaPath, { withFileTypes: true });
        } catch { return; }

        // Process model folders in parallel
        const modelPromises = modelEntries
          .filter(e => e.isDirectory())
          .map(async (modelEntry) => {
            const modelPath = path.join(alphaPath, modelEntry.name);
            let setEntries;
            try {
              setEntries = await fs.promises.readdir(modelPath, { withFileTypes: true });
            } catch { return; }

            for (const setEntry of setEntries) {
              if (!setEntry.isDirectory()) continue;
              const setPath = path.join(modelPath, setEntry.name);
              const parsed = parseSetFolder(setEntry.name);

              sets.push({
                id: setPath,
                name: setEntry.name,
                path: setPath,
                tag,
                category: parsed.category,
                site: parsed.site,
                modelName: modelEntry.name,
                photoCount: -1,
                eligible: true,
              });
            }
          });

        await Promise.all(modelPromises);
      });

    await Promise.all(alphaPromises);
  }

  return sets;
}

async function scanVideoFolders() {
  const videos = [];
  const videoExts = ['.mp4', '.mkv', '.avi', '.mov', '.wmv', '.webm'];

  for (const videoDir of VIDEO_DIRS) {
    let topEntries;
    try { topEntries = await fs.promises.readdir(videoDir); } catch { continue; }

    for (const topEntry of topEntries) {
      const topPath = path.join(videoDir, topEntry);
      let topStat;
      try { topStat = await fs.promises.stat(topPath); } catch { continue; }
      if (!topStat.isDirectory()) continue;

      // Check if this is a model folder (contains videos directly)
      // or a sub-volume folder (contains model folders)
      let entries;
      try { entries = await fs.promises.readdir(topPath); } catch { continue; }

      const hasVideos = entries.some(e => videoExts.includes(path.extname(e).toLowerCase()));

      // Parse video filenames using same approach as photo sets
      function collectVideoMeta(videoFiles) {
        const categories = new Set();
        const sites = new Set();
        for (const f of videoFiles) {
          const parsed = parseSetFolder(path.basename(f, path.extname(f)));
          if (parsed.category !== 'unknown') categories.add(parsed.category);
          if (parsed.site !== 'unknown') sites.add(parsed.site);
        }
        return { categories: [...categories], sites: [...sites] };
      }

      if (hasVideos) {
        const videoFiles = entries.filter(f => videoExts.includes(path.extname(f).toLowerCase()));
        if (videoFiles.length > 0) {
          const meta = collectVideoMeta(videoFiles);
          videos.push({
            id: topPath, name: topEntry, path: topPath,
            tag: 'unknown', tags: [],
            categories: meta.categories, sites: meta.sites,
            category: meta.categories[0] || 'unknown',
            site: meta.sites[0] || 'unknown',
            videoCount: videoFiles.length, type: 'folder', eligible: true,
          });
        }
      } else {
        for (const modelName of entries) {
          const modelPath = path.join(topPath, modelName);
          let modelStat;
          try { modelStat = await fs.promises.stat(modelPath); } catch { continue; }
          if (!modelStat.isDirectory()) continue;

          let modelEntries;
          try { modelEntries = await fs.promises.readdir(modelPath); } catch { continue; }
          const modelVideos = modelEntries.filter(f => videoExts.includes(path.extname(f).toLowerCase()));

          if (modelVideos.length > 0) {
            const meta = collectVideoMeta(modelVideos);
            videos.push({
              id: modelPath, name: modelName, path: modelPath,
              tag: 'unknown', tags: [],
              categories: meta.categories, sites: meta.sites,
              category: meta.categories[0] || 'unknown',
              site: meta.sites[0] || 'unknown',
              videoCount: modelVideos.length, type: 'folder', eligible: true,
            });
          }
        }
      }
    }
  }

  return videos;
}

// ── Finder tag scanning ──

const FINDER_TAGS = ['Red', 'Orange', 'Yellow', 'Green', 'Blue', 'Purple', 'Gray', '💄'];

function mdfindByTag(tag, basePath) {
  return new Promise((resolve, reject) => {
    execFile('mdfind', ['-onlyin', basePath, `kMDItemUserTags == "${tag}"`],
      { maxBuffer: 50 * 1024 * 1024 },
      (error, stdout) => {
        if (error) return reject(error);
        // Extract parent folder names (model folders)
        const folders = new Set();
        stdout.trim().split('\n').filter(Boolean).forEach(f => {
          const dir = path.dirname(f);
          folders.add(dir);
        });
        resolve([...folders]);
      }
    );
  });
}

let finderTagCache = null;
let finderTagCacheTime = 0;

async function getFinderTagMap() {
  const now = Date.now();
  if (finderTagCache && (now - finderTagCacheTime) < 10 * 60 * 1000) return finderTagCache;

  console.log('Scanning Finder tags...');
  const tagMap = {}; // folder path → [tags]
  for (const tag of FINDER_TAGS) {
    for (const videoDir of VIDEO_DIRS) {
      try {
        const folders = await mdfindByTag(tag, videoDir);
        for (const folder of folders) {
          if (!tagMap[folder]) tagMap[folder] = [];
          if (!tagMap[folder].includes(tag)) tagMap[folder].push(tag);
        }
      } catch { /* skip */ }
    }
  }
  finderTagCache = tagMap;
  finderTagCacheTime = now;
  console.log(`Finder tag map: ${Object.keys(tagMap).length} folders tagged`);
  return tagMap;
}

// Endpoint to get video tags
app.get('/api/video-tags', async (_req, res) => {
  const tagMap = await getFinderTagMap();
  res.json(tagMap);
});

// ── Cache ──

let photoCache = null;
let videoCahe = null;
let photoCacheTime = 0;
let videoCacheTime = 0;
const CACHE_TTL = 5 * 60 * 1000; // 5 minutes

// ── API Routes ──

app.get('/api/photos', async (_req, res) => {
  const now = Date.now();
  if (!photoCache || (now - photoCacheTime) > CACHE_TTL) {
    console.log('Scanning photo sets...');
    photoCache = await scanPhotoSets();
    photoCacheTime = now;
    console.log(`Found ${photoCache.length} photo sets`);
  }
  res.json(photoCache);
});

app.get('/api/videos', async (_req, res) => {
  const now = Date.now();
  if (!videoCahe || (now - videoCacheTime) > CACHE_TTL) {
    console.log('Scanning video folders...');
    videoCahe = await scanVideoFolders();
    // Merge Finder tags
    try {
      const tagMap = await getFinderTagMap();
      for (const v of videoCahe) {
        const tags = tagMap[v.path] || [];
        v.tag = tags.length > 0 ? tags[0] : 'unknown';
        v.tags = tags;
      }
    } catch (e) {
      console.log('Finder tag scan failed, continuing without tags');
    }
    videoCacheTime = now;
    console.log(`Found ${videoCahe.length} video entries`);
  }
  res.json(videoCahe);
});

// Count photos in a set (lazy)
app.get('/api/photos/count', async (req, res) => {
  const setPath = req.query.path;
  if (!setPath) return res.status(400).json({ error: 'Missing path' });
  const allowed = PHOTO_SET_PARENTS.some(p => setPath.startsWith(p));
  if (!allowed) return res.status(403).json({ error: 'Path not allowed' });
  const count = await countFiles(setPath);
  res.json({ count, eligible: count > 0 });
});

// List photos in a set folder
app.get('/api/photos/set', async (req, res) => {
  const setPath = req.query.path;
  if (!setPath) return res.status(400).json({ error: 'Missing path' });

  // Validate path is under allowed parents
  const allowed = PHOTO_SET_PARENTS.some(p => setPath.startsWith(p));
  if (!allowed) return res.status(403).json({ error: 'Path not allowed' });

  try {
    const entries = await fs.promises.readdir(setPath);
    const imageExts = ['.jpg', '.jpeg', '.png', '.gif', '.webp'];
    const photos = entries
      .filter(e => imageExts.includes(path.extname(e).toLowerCase()))
      .sort()
      .map(filename => ({
        filename,
        // Construct URL relative to MEDIA_BASE_PATH for images-api-video
        // The set is inside photo sets parent → alpha → setName
        // images-api serves from /Volumes/VideosNew/Models, so we need full path via proxy
        url: `/api/proxy/image?path=${encodeURIComponent(path.join(setPath, filename))}`,
      }));
    res.json(photos);
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

// List videos in a folder
app.get('/api/videos/list', async (req, res) => {
  const folderPath = req.query.path;
  if (!folderPath) return res.status(400).json({ error: 'Missing path' });

  const allowed = VIDEO_DIRS.some(p => folderPath.startsWith(p));
  if (!allowed) return res.status(403).json({ error: 'Path not allowed' });

  try {
    const stat = await fs.promises.stat(folderPath);
    if (!stat.isDirectory()) {
      // Single file
      return res.json([{
        filename: path.basename(folderPath),
        url: `/api/proxy/video?path=${encodeURIComponent(folderPath)}`,
      }]);
    }

    const entries = await fs.promises.readdir(folderPath);
    const videoExts = ['.mp4', '.mkv', '.avi', '.mov', '.wmv', '.webm'];
    const videos = entries
      .filter(e => videoExts.includes(path.extname(e).toLowerCase()))
      .sort()
      .map(filename => ({
        filename,
        url: `/api/proxy/video?path=${encodeURIComponent(path.join(folderPath, filename))}`,
      }));
    res.json(videos);
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

// Proxy image serving (direct file serve for speed)
app.get('/api/proxy/image', (req, res) => {
  const filePath = req.query.path;
  if (!filePath) return res.status(400).json({ error: 'Missing path' });

  const allowed = PHOTO_SET_PARENTS.some(p => filePath.startsWith(p));
  if (!allowed) return res.status(403).json({ error: 'Path not allowed' });

  const resolved = path.resolve(filePath);
  if (!PHOTO_SET_PARENTS.some(p => resolved.startsWith(p))) {
    return res.status(403).json({ error: 'Path traversal blocked' });
  }

  const ext = path.extname(filePath).toLowerCase();
  const types = { '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.png': 'image/png', '.gif': 'image/gif', '.webp': 'image/webp' };
  const contentType = types[ext] || 'application/octet-stream';

  try {
    const stat = fs.statSync(filePath);
    res.setHeader('Content-Type', contentType);
    res.setHeader('Content-Length', stat.size);
    res.setHeader('Cache-Control', 'public, max-age=86400');
    fs.createReadStream(filePath).pipe(res);
  } catch {
    res.status(404).json({ error: 'File not found' });
  }
});

// Proxy video streaming — direct file serve for speed (same machine)
app.get('/api/proxy/video', (req, res) => {
  const filePath = req.query.path;
  if (!filePath) return res.status(400).json({ error: 'Missing path' });

  const allAllowed = [...VIDEO_DIRS, ...PHOTO_SET_PARENTS];
  const resolved = path.resolve(filePath);
  if (!allAllowed.some(p => resolved.startsWith(p))) {
    return res.status(403).json({ error: 'Path not allowed' });
  }

  let stat;
  try { stat = fs.statSync(filePath); } catch {
    return res.status(404).json({ error: 'File not found' });
  }

  const fileSize = stat.size;
  const range = req.headers.range;
  const ext = path.extname(filePath).toLowerCase();
  const videoTypes = { '.mp4': 'video/mp4', '.mkv': 'video/x-matroska', '.avi': 'video/x-msvideo', '.mov': 'video/quicktime', '.wmv': 'video/x-ms-wmv', '.webm': 'video/webm' };
  const contentType = videoTypes[ext] || 'video/mp4';

  if (range) {
    const parts = range.replace(/bytes=/, '').split('-');
    const start = parseInt(parts[0], 10);
    const end = parts[1] ? parseInt(parts[1], 10) : fileSize - 1;
    const chunkSize = end - start + 1;

    res.writeHead(206, {
      'Content-Range': `bytes ${start}-${end}/${fileSize}`,
      'Accept-Ranges': 'bytes',
      'Content-Length': chunkSize,
      'Content-Type': contentType,
    });
    fs.createReadStream(filePath, { start, end }).pipe(res);
  } else {
    res.writeHead(200, {
      'Content-Length': fileSize,
      'Content-Type': contentType,
      'Accept-Ranges': 'bytes',
    });
    fs.createReadStream(filePath).pipe(res);
  }
});

// ── Playlist persistence ──
// Store lives in lib/playlist-store.js (atomic tmp+rename writes, path-keyed
// add/remove). Per-item endpoints keep request bodies tiny regardless of
// playlist size; the whole-blob POST remains for clear/reorder.

app.get('/api/playlists', (_req, res) => {
  res.json(loadPlaylists(PLAYLIST_FILE));
});

app.post('/api/playlists', (req, res) => {
  try {
    savePlaylists(PLAYLIST_FILE, req.body);
    res.json({ ok: true });
  } catch (err) {
    res.status(500).json({ ok: false, error: err.message });
  }
});

app.post('/api/playlists/add', (req, res) => {
  const { type, item } = req.body || {};
  try {
    const data = addItem(loadPlaylists(PLAYLIST_FILE), type, item);
    savePlaylists(PLAYLIST_FILE, data);
    res.json({ ok: true, photos: data.photos.length, videos: data.videos.length });
  } catch (err) {
    res.status(400).json({ ok: false, error: err.message });
  }
});

app.post('/api/playlists/remove', (req, res) => {
  const { type, path: itemPath } = req.body || {};
  try {
    const data = removeItem(loadPlaylists(PLAYLIST_FILE), type, itemPath);
    savePlaylists(PLAYLIST_FILE, data);
    res.json({ ok: true, photos: data.photos.length, videos: data.videos.length });
  } catch (err) {
    res.status(400).json({ ok: false, error: err.message });
  }
});

// ── Serve index ──
app.get('/', (_req, res) => {
  res.sendFile(path.join(__dirname, 'public', 'index.html'));
});

app.listen(PORT, '0.0.0.0', () => {
  console.log(`Sexuality player running on http://0.0.0.0:${PORT}`);
});
