/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Catalog scanner walks user-configured SOURCE_DIRS by design; paths are
 * built from the validated SOURCE_DIRS table, not user input. Dynamic
 * fs reads are intentional. Lift-and-shift from gathering.
 */
/**
 * Doc-catalog handler — relocated from jeff-bridwell-personal-site (#2445).
 *
 * Lift-and-shift from gathering's src/handlers/doc-catalog.handler.ts. The
 * catalog is meta-tooling about the corpus, not a feature of either product;
 * its honest home is chorus-api.
 *
 * Endpoints:
 *   GET  /api/doc-catalog                — list all docs grouped by topic
 *   POST /api/doc-catalog/add            — register a new doc
 *   GET  /api/doc-catalog/domain/:domain — domain-scoped docs
 *   POST /api/doc-catalog/link           — link a doc to a domain
 *
 * Architecture: pure functions (buildDocCatalog, registerDoc, linkDocToDomain,
 * getDomainArtifacts) return { status, body }. Express adapters call them and
 * write to res. Tests import the pure functions directly.
 */

import type { Request, Response } from 'express';
import fs from 'fs';
import path from 'path';

// Read env at every call so tests can override via process.env at runtime
// (#2517 hermetic-fixture variant).
function gatheringRoot(): string {
  return process.env.GATHERING_REPO || '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site';
}
function chorusRoot(): string {
  return process.env.CHORUS_REPO || '/Users/jeffbridwell/CascadeProjects/chorus';
}
function registryPath(): string {
  return path.join(chorusRoot(), 'platform', 'api', 'data', 'doc-catalog-registry.json');
}
function linksPath(): string {
  return path.join(chorusRoot(), 'platform', 'api', 'data', 'doc-catalog-links.json');
}

type ArtifactType = 'service-design' | 'decision' | 'adr' | 'architecture' | 'research' | 'manual' | 'ontology' | 'domain-page' | 'product' | 'process';

export interface DocEntry {
  title: string;
  href: string;
  source: string;
  group: string;
  artifactType: ArtifactType;
  date: string;
  sizeKB: number | null;
}

export interface DocGroup { name: string; docs: DocEntry[]; }

interface SourceDir {
  root: 'gathering' | 'chorus';
  dir: string;
  urlPrefix: string;
  source: string;
  defaultGroup: string;
}

interface RegisteredDoc { filePath: string; href: string; group?: string; }
interface DomainLink { href: string; domain: string; relationship: 'governs' | 'references'; }

export interface DocCatalogResult {
  totalDocs: number;
  groups: DocGroup[];
}

export interface HandlerResult {
  status: number;
  body: unknown;
}

const SOURCE_DIRS: SourceDir[] = [
  { root: 'gathering', dir: 'public', urlPrefix: '/', source: 'public', defaultGroup: 'Product Strategy & Vision' },
  { root: 'gathering', dir: 'docs', urlPrefix: '/docs/', source: 'docs', defaultGroup: 'Architecture & System Design' },
  { root: 'gathering', dir: 'data/about', urlPrefix: '/system/docs/', source: 'data/about', defaultGroup: 'Engineering & Quality' },
  { root: 'gathering', dir: 'public/gathering-docs', urlPrefix: '/gathering-docs/', source: 'gathering-docs', defaultGroup: 'Product Strategy & Vision' },
  { root: 'gathering', dir: 'public/akasha', urlPrefix: '/akasha/', source: 'akasha', defaultGroup: 'Akasha — Consulting Site' },
  { root: 'gathering', dir: 'public/chorus-docs', urlPrefix: '/chorus-docs/', source: 'chorus-docs', defaultGroup: 'Chorus & Team Coordination' },
  { root: 'chorus', dir: 'roles/wren/artifacts', urlPrefix: '/gathering-docs/', source: 'wren/artifacts', defaultGroup: 'Product Strategy & Vision' },
  { root: 'chorus', dir: 'roles/wren/docs', urlPrefix: '/gathering-docs/', source: 'wren/docs', defaultGroup: 'Product Strategy & Vision' },
  { root: 'chorus', dir: 'roles/wren/decisions', urlPrefix: '/docs/decisions/', source: 'wren/decisions', defaultGroup: 'Architecture & Design' },
  { root: 'chorus', dir: 'roles/silas/docs', urlPrefix: '/docs/architect/', source: 'architect/docs', defaultGroup: 'Architecture & System Design' },
  { root: 'chorus', dir: 'roles/silas/artifacts', urlPrefix: '/docs/architect/', source: 'architect/artifacts', defaultGroup: 'Architecture & System Design' },
  { root: 'chorus', dir: 'roles/silas/adr', urlPrefix: '/docs/adr/', source: 'architect/adr', defaultGroup: 'Architecture & Design' },
  { root: 'chorus', dir: 'designing/docs', urlPrefix: '/docs/designing/', source: 'designing/docs', defaultGroup: 'Architecture & System Design' },
  { root: 'chorus', dir: 'designing/decisions', urlPrefix: '/docs/decisions/', source: 'designing/decisions', defaultGroup: 'Architecture & Design' },
  { root: 'chorus', dir: 'docs/diagrams', urlPrefix: '/docs/diagrams/', source: 'docs/diagrams', defaultGroup: 'Architecture & Design' },
];

function rootPath(root: 'gathering' | 'chorus'): string {
  return root === 'gathering' ? gatheringRoot() : chorusRoot();
}

function extractTitle(filePath: string, filename: string): string {
  try {
    const head = fs.readFileSync(filePath, 'utf-8').slice(0, 2000);
    if (filename.endsWith('.md')) {
      const match = head.match(/^#\s+(.+)$/m);
      if (match) return match[1].trim();
    } else {
      const match = head.match(/<title[^>]*>([^<]+)<\/title>/i);
      if (match) return match[1].replace(/\s*[—–|]\s*Gathering.*$/i, '').trim();
    }
  } catch { /* fall through */ }
  return filename
    .replace(/\.(html|md)$/, '')
    .replace(/[_-]/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase());
}

function classifyArtifactType(title: string, filename: string): ArtifactType {
  const t = (title + ' ' + filename).toLowerCase();
  if (/service.design/.test(t)) return 'service-design';
  if (/^dec-|decision/.test(t)) return 'decision';
  if (/^adr-|architecture.decision.record/.test(t)) return 'adr';
  if (/^domain\s*[—–-]\s/.test(title)) return 'domain-page';
  if (/ontology|owl|rdf|predicate|class.diagram/.test(t)) return 'ontology';
  if (/c4|architecture|system.model|hexagonal|convergence.arch|memory.arch|attention.arch|protocol.stack|nervous.system/.test(t)) return 'architecture';
  if (/research|homeostasis|position|explained.for.humans/.test(t)) return 'research';
  if (/manual|user.manual|definition.of.done|playbook/.test(t)) return 'manual';
  if (/product|vision|roadmap|positioning|business.plan|conceptual.model/.test(t)) return 'product';
  if (/demo.plan|demo.flow|pair.flow|process|workflow|gemba/.test(t)) return 'process';
  return 'architecture';
}

// eslint-disable-next-line complexity
function classifyGroup(title: string, filename: string): string {
  const t = (title + ' ' + filename).toLowerCase();
  if (/^Domain\s*[—–-]\s/.test(title)) return 'Domains';
  if (/sequence.diagram|sequence-/.test(t)) return 'Sequence Diagrams';
  if (/actor.diagram/.test(t)) return 'Actor Diagrams';
  if (/service.design/.test(t)) return 'Service Designs';
  if (/manual|user.manual|definition.of.done/.test(t)) return 'Manuals & Guides';
  if (/borg|structural.audit|self.assessment|dpor|assimilation/.test(t)) return 'Borg Assessments';
  if (/wardley/.test(t)) return 'Wardley Maps';
  if (/research|homeostasis|position|explained.for.humans/.test(t)) return 'Research';
  if (/ontology|semantic|owl|rdf|predicate|class.diagram/.test(t)) return 'Ontology & Semantics';
  if (/c4|architecture|system.model|hexagonal|tech.stack|interaction.arch|convergence.arch|memory.arch|attention.arch|protocol.stack|nervous.system|rebuild/.test(t)) return 'Architecture & Design';
  if (/icd|harvest|reconcil|ingest|mapper|merge.spec|source.semantic|source.rich|etl|pipeline.comparison/.test(t)) return 'ICD & Convergence';
  if (/photo|face.cluster|thumbnail|era.scoped/.test(t)) return 'Photos';
  if (/chorus.*spine|clearing|nudge|bridge|messaging|card.lifecycle|command.card|werk|card.type|hook|pulse|team.aware/.test(t)) return 'Chorus Coordination';
  if (/product|vision|roadmap|positioning|business.plan|topology|value.stream|genome|conceptual.model|model.driven|deep.dive/.test(t)) return 'Product Strategy';
  if (/analytic|cadence|cost|metric|insight|voice|attention.*intensity|re.prompt/.test(t)) return 'Analytics';
  if (/infra|log.topology|fuseki|docker|launchagent|deploy|tunnel|disk|home.cloud|logging.strategy/.test(t)) return 'Infrastructure';
  if (/garden|property|lightlife|urban|basement/.test(t)) return 'Garden & Property';
  if (/akasha|consult/.test(t)) return 'Consulting';
  if (/self.domain|self.portrait|people.*relationship|spine.*jeff|kade.*engineer|silas.*architect/.test(t)) return 'People & Self';
  if (/demo.plan|demo.flow|pair.flow|next.queue|triage|gemba|playbook|process/.test(t)) return 'Process & Workflow';
  if (/bdd|gherkin|test.automation|gate.test/.test(t)) return 'BDD & Testing';
  return 'Other';
}

const GROUP_ORDER = [
  'Domains', 'Service Designs', 'Manuals & Guides', 'Architecture & Design',
  'Ontology & Semantics', 'Product Strategy', 'Chorus Coordination',
  'ICD & Convergence', 'Process & Workflow', 'Sequence Diagrams',
  'Actor Diagrams', 'Wardley Maps', 'Borg Assessments', 'BDD & Testing',
  'Research', 'Analytics', 'Photos', 'Infrastructure', 'People & Self',
  'Garden & Property', 'Consulting', 'Other',
];

function buildGroups(allDocs: DocEntry[]): DocGroup[] {
  const groupMap = new Map<string, DocEntry[]>();
  for (const doc of allDocs) {
    if (!groupMap.has(doc.group)) groupMap.set(doc.group, []);
    groupMap.get(doc.group)!.push(doc);
  }
  groupMap.forEach(docs => docs.sort((a, b) => a.title.localeCompare(b.title)));
  const groups: DocGroup[] = [];
  for (const name of GROUP_ORDER) {
    if (groupMap.has(name)) {
      groups.push({ name, docs: groupMap.get(name)! });
      groupMap.delete(name);
    }
  }
  groupMap.forEach((docs, name) => groups.push({ name, docs }));
  return groups;
}

function addOrReplace(allDocs: DocEntry[], seenTitle: Map<string, DocEntry>, doc: DocEntry): void {
  const existing = seenTitle.get(doc.title);
  if (existing) {
    if (doc.date > existing.date) {
      const idx = allDocs.indexOf(existing);
      if (idx !== -1) allDocs[idx] = doc;
      seenTitle.set(doc.title, doc);
    }
    return;
  }
  seenTitle.set(doc.title, doc);
  allDocs.push(doc);
}

function scanDirectory(sd: SourceDir): DocEntry[] {
  const absDir = path.join(rootPath(sd.root), sd.dir);
  if (!fs.existsSync(absDir)) return [];
  const entries: DocEntry[] = [];
  const files = fs.readdirSync(absDir).filter(f => f.endsWith('.html') || f.endsWith('.md'));
  for (const f of files) {
    const filePath = path.join(absDir, f);
    let stat: fs.Stats;
    try { stat = fs.statSync(filePath); } catch { continue; }
    const title = extractTitle(filePath, f);
    const isSlugRoute = sd.urlPrefix === '/system/docs/' || sd.urlPrefix === '/docs/';
    const slugPart = isSlugRoute ? f.replace(/\.(md|html)$/i, '') : f;
    const href = sd.urlPrefix + slugPart;
    const group = classifyGroup(title, f);
    const artifactType = classifyArtifactType(title, f);
    const date = stat.mtime.toISOString().slice(0, 10);
    const sizeKB = Math.round(stat.size / 1024) || null;
    entries.push({ title, href, source: sd.source, group, artifactType, date, sizeKB });
  }
  return entries;
}

function loadRegistered(): RegisteredDoc[] {
  try {
    if (fs.existsSync(registryPath())) return JSON.parse(fs.readFileSync(registryPath(), 'utf-8'));
  } catch { /* corrupt */ }
  return [];
}

function saveRegistered(docs: RegisteredDoc[]): void {
  fs.mkdirSync(path.dirname(registryPath()), { recursive: true });
  fs.writeFileSync(registryPath(), JSON.stringify(docs, null, 2));
}

function registeredToEntry(reg: RegisteredDoc): DocEntry | null {
  if (!fs.existsSync(reg.filePath)) return null;
  let stat: fs.Stats;
  try { stat = fs.statSync(reg.filePath); } catch { return null; }
  const filename = path.basename(reg.filePath);
  const title = extractTitle(reg.filePath, filename);
  const group = reg.group || classifyGroup(title, filename);
  const artifactType = classifyArtifactType(title, filename);
  const date = stat.mtime.toISOString().slice(0, 10);
  const sizeKB = Math.round(stat.size / 1024) || null;
  return { title, href: reg.href, source: 'manual', group, artifactType, date, sizeKB };
}

function loadLinks(): DomainLink[] {
  try {
    if (fs.existsSync(linksPath())) return JSON.parse(fs.readFileSync(linksPath(), 'utf-8'));
  } catch { /* corrupt */ }
  return [];
}

function saveLinks(links: DomainLink[]): void {
  fs.mkdirSync(path.dirname(linksPath()), { recursive: true });
  fs.writeFileSync(linksPath(), JSON.stringify(links, null, 2));
}

function inferDomainLinks(docs: DocEntry[]): DomainLink[] {
  const links: DomainLink[] = [];
  for (const doc of docs) {
    const filename = path.basename(doc.href);
    const sdMatch = filename.match(/^service-design-(.+)\.html$/);
    if (sdMatch) { links.push({ href: doc.href, domain: sdMatch[1], relationship: 'governs' }); continue; }
    const domMatch = filename.match(/^domain-(.+)\.html$/);
    if (domMatch) { links.push({ href: doc.href, domain: domMatch[1], relationship: 'governs' }); }
  }
  return links;
}

function collectDocs(sourceDirs: SourceDir[] = SOURCE_DIRS): DocEntry[] {
  const allDocs: DocEntry[] = [];
  const seenHref = new Set<string>();
  const seenTitle = new Map<string, DocEntry>();
  for (const sd of sourceDirs) {
    for (const doc of scanDirectory(sd)) {
      if (seenHref.has(doc.href)) continue;
      seenHref.add(doc.href);
      addOrReplace(allDocs, seenTitle, doc);
    }
  }
  for (const reg of loadRegistered()) {
    if (seenHref.has(reg.href)) continue;
    const entry = registeredToEntry(reg);
    if (entry) {
      seenHref.add(entry.href);
      if (!seenTitle.has(entry.title)) {
        seenTitle.set(entry.title, entry);
        allDocs.push(entry);
      }
    }
  }
  return allDocs;
}

// --- Pure functions (testable, no Express) ---

export function buildDocCatalog(sourceDirs?: SourceDir[]): DocCatalogResult {
  const allDocs = collectDocs(sourceDirs);
  const groups = buildGroups(allDocs);
  return { totalDocs: allDocs.length, groups };
}

// Exported for hermetic tests (#2517 AC3)
export type { SourceDir };

export function registerDoc(input: { filePath?: string; href?: string; group?: string }): HandlerResult {
  const { filePath, href, group } = input;
  if (!filePath || !href) return { status: 400, body: { error: 'Required: filePath and href' } };
  const absPath = path.resolve(filePath);
  if (!fs.existsSync(absPath)) return { status: 404, body: { error: `File not found: ${absPath}` } };
  if (!absPath.endsWith('.html') && !absPath.endsWith('.md')) {
    return { status: 400, body: { error: 'Only .html and .md files can be registered' } };
  }
  const registered = loadRegistered();
  if (registered.some(r => r.href === href)) {
    return { status: 409, body: { error: `Already registered: ${href}` } };
  }
  const entry: RegisteredDoc = { filePath: absPath, href, group };
  registered.push(entry);
  saveRegistered(registered);
  return { status: 201, body: { registered: entry, doc: registeredToEntry(entry) } };
}

export function getDomainArtifacts(domain: string | undefined): HandlerResult {
  if (!domain) return { status: 400, body: { error: 'Missing domain parameter' } };
  const allDocs = collectDocs();
  const manualLinks = loadLinks();
  const autoLinks = inferDomainLinks(allDocs);
  const seen = new Set<string>();
  const uniqueLinks = [...manualLinks, ...autoLinks].filter(l => {
    const key = `${l.href}|${l.domain}|${l.relationship}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  const domainLinks = uniqueLinks.filter(l => l.domain === domain);
  const docMap = new Map(allDocs.map(d => [d.href, d]));
  const governs = domainLinks.filter(l => l.relationship === 'governs').map(l => docMap.get(l.href)).filter(Boolean) as DocEntry[];
  const references = domainLinks.filter(l => l.relationship === 'references').map(l => docMap.get(l.href)).filter(Boolean) as DocEntry[];
  const total = governs.length + references.length;
  const thirtyDaysAgo = new Date(); thirtyDaysAgo.setDate(thirtyDaysAgo.getDate() - 30);
  const stale = [...governs, ...references].filter(d => new Date(d.date) < thirtyDaysAgo).length;
  return {
    status: 200,
    body: {
      domain,
      governs: governs.map(d => ({ title: d.title, type: d.artifactType, href: d.href, modified: d.date })),
      references: references.map(d => ({ title: d.title, type: d.artifactType, href: d.href, modified: d.date })),
      health: { total, stale, brokenLinks: 0, undocumented: total === 0 },
    },
  };
}

export function linkDocToDomain(input: { href?: string; domain?: string; relationship?: string }): HandlerResult {
  const { href, domain, relationship } = input;
  if (!href || !domain || !relationship) return { status: 400, body: { error: 'Required: href, domain, relationship' } };
  if (relationship !== 'governs' && relationship !== 'references') {
    return { status: 400, body: { error: 'relationship must be "governs" or "references"' } };
  }
  const links = loadLinks();
  if (links.some(l => l.href === href && l.domain === domain && l.relationship === relationship)) {
    return { status: 409, body: { error: 'Link already exists' } };
  }
  const link: DomainLink = { href, domain, relationship: relationship as 'governs' | 'references' };
  links.push(link);
  saveLinks(links);
  return { status: 201, body: { linked: link } };
}

// --- Express adapters ---

export function listCatalog(_req: Request, res: Response): void {
  try {
    const result = buildDocCatalog();
    res.json(result);
  } catch (error) {
    console.error('[doc-catalog] listCatalog failed', error);
    res.status(500).json({ error: 'Failed to scan doc catalog' });
  }
}

export function addDoc(req: Request, res: Response): void {
  try {
    const r = registerDoc(req.body || {});
    res.status(r.status).json(r.body);
  } catch (error) {
    console.error('[doc-catalog] addDoc failed', error);
    res.status(500).json({ error: 'Failed to register doc' });
  }
}

export function domainArtifacts(req: Request, res: Response): void {
  try {
    const r = getDomainArtifacts(req.params.domain);
    res.status(r.status).json(r.body);
  } catch (error) {
    console.error('[doc-catalog] domainArtifacts failed', error);
    res.status(500).json({ error: 'Failed to get domain artifacts' });
  }
}

export function linkArtifact(req: Request, res: Response): void {
  try {
    const r = linkDocToDomain(req.body || {});
    res.status(r.status).json(r.body);
  } catch (error) {
    console.error('[doc-catalog] linkArtifact failed', error);
    res.status(500).json({ error: 'Failed to link artifact' });
  }
}
