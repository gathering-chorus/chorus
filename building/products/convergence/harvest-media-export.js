// harvest-media-export.js — mongosh script: MongoDB content → Turtle
// Card: #376 | Arch: volume-sharded collection graphs
// Expects: VOLUME variable set before execution (e.g., "VideosNew")
// Outputs: Turtle to stdout

// Prefixes
print('@prefix jb: <https://jeffbridwell.com/ontology#> .');
print('@prefix dc: <http://purl.org/dc/terms/> .');
print('@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .');
print('');

const BASE = 'https://jeffbridwell.com/pods/jeff/media/';
const CHUNK_SIZE = typeof CHUNK !== 'undefined' ? CHUNK : 0;
const CHUNK_OFFSET = typeof OFFSET !== 'undefined' ? OFFSET : 0;

function esc(s) {
  if (!s) return '';
  return String(s)
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '');
}

function rdfClass(ct) {
  if (!ct) return 'jb:MediaItem';
  ct = ct.toLowerCase();
  if (ct.startsWith('image/')) return 'jb:MediaPhoto';
  if (ct.startsWith('video/')) return 'jb:Video';
  if (ct === 'application/zip') return 'jb:MediaArchive';
  return 'jb:MediaItem';
}

function parseDate(doc) {
  const mdls = doc.extended_attributes && doc.extended_attributes.mdls;
  if (mdls && mdls.kMDItemContentCreationDate) {
    const raw = mdls.kMDItemContentCreationDate;
    // "2024-02-13 23:33:26 +0000" → "2024-02-13T23:33:26Z"
    const cleaned = raw.replace(/\s\+\d{4}$/, 'Z').replace(' ', 'T');
    // Validate — must look like a dateTime
    if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(cleaned)) return cleaned;
  }
  const created = doc.base_attributes && doc.base_attributes.created;
  if (created && typeof created === 'number') {
    try { return new Date(created * 1000).toISOString(); } catch(e) { /* skip */ }
  }
  return null;
}

// Build regex for this volume
const escapedVol = VOLUME.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const pattern = '^/[^/]+/' + escapedVol + '/';

let cursor = db.content.find(
  { file_path: { $regex: pattern } },
  { file_path: 1, content_type: 1, base_attributes: 1, extended_attributes: 1 }
).sort({ _id: 1 });

if (CHUNK_OFFSET > 0) cursor = cursor.skip(CHUNK_OFFSET);
if (CHUNK_SIZE > 0) cursor = cursor.limit(CHUNK_SIZE);

let count = 0;
cursor.forEach(doc => {
  const id = doc._id.toString();
  const parts = doc.file_path.split('/');
  const filename = parts[parts.length - 1];
  const cls = rdfClass(doc.content_type);
  const size = (doc.base_attributes && doc.base_attributes.size) || 0;
  const date = parseDate(doc);

  let props = [];
  props.push('    a ' + cls);
  props.push('    jb:photoFilename "' + esc(filename) + '"');
  props.push('    jb:filePath "' + esc(doc.file_path) + '"');
  if (doc.content_type) props.push('    dc:format "' + esc(doc.content_type) + '"');
  if (size > 0) props.push('    jb:fileSize ' + size);
  if (date) props.push('    dc:created "' + date + '"^^xsd:dateTime');
  props.push('    jb:sourceVolume "' + esc(VOLUME) + '"');

  print('<' + BASE + id + '>');
  print(props.join(' ;\n') + ' .\n');

  count++;
  if (count % 100000 === 0) {
    print('# Progress: ' + count + ' items');
  }
});

print('# Total: ' + count + ' items for volume ' + VOLUME);
