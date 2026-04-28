/* eslint-disable security/detect-object-injection --
 * SUBDOMAIN_TO_SUBPRODUCT and tag dictionaries are keyed by validated
 * domain names from a fixed enum; not user input. Lookups are safe.
 */
/**
 * doc-tagger — infer Athena ontology tags for catalog docs (#2520).
 *
 * Maps a doc (path + filename + optional frontmatter) to:
 *   product:    chorus | gathering | consulting
 *   subproduct: loom | werk | athena | convergence | clearing | quality (chorus only)
 *   subdomain:  one of Athena's 48 subdomains (e.g., loom-decisions, blog-domain)
 *
 * Inference order: frontmatter override > path-based > filename pattern > content.
 * Returns confidence: high (path or frontmatter) | medium (filename pattern) |
 *                     low (content keywords) | none (no match).
 */

export interface DocTagInput {
  sourcePath: string;
  basename: string;
  contentHead?: string;
  frontmatter?: { product?: string; subproduct?: string; subdomain?: string };
}

export interface DocTags {
  product?: string;
  subproduct?: string;
  subdomain?: string;
  confidence: 'high' | 'medium' | 'low' | 'none';
  signal: 'frontmatter' | 'path' | 'filename' | 'content' | 'none';
}

// Subproduct → subdomain ownership (from Athena #subproducts and #subdomains).
export const SUBPRODUCT_DOMAINS: Record<string, string[]> = {
  loom: [
    'loom-analytics', 'loom-decisions', 'loom-metrics', 'loom-policies',
    'loom-practices', 'loom-principles', 'loom-rcas',
  ],
  werk: ['cards-service', 'roles-domain'],
  athena: ['athena-domain', 'domains-domain', 'knowledge-domain', 'integrations-domain', 'services-domain'],
  convergence: ['convergence-domain'],
  clearing: ['messages-domain'],
  quality: ['gates-service', 'tests-domain'],
};

const SUBDOMAIN_TO_SUBPRODUCT: Record<string, string> = {};
for (const [sp, doms] of Object.entries(SUBPRODUCT_DOMAINS)) {
  for (const d of doms) SUBDOMAIN_TO_SUBPRODUCT[d] = sp;
}

// Gathering subdomains (no subproduct level)
export const GATHERING_SUBDOMAINS = new Set([
  'blog-domain', 'books-domain', 'cooking-domain', 'documents-domain',
  'glimmers-domain', 'music-domain', 'notes-domain', 'photos-domain',
  'property-domain', 'social-domain', 'video-domain',
  'people-domain', 'sexuality-domain', 'stories-domain', 'self-domain',
  'seeds-domain', 'reading-domain', 'watching-domain',
]);

// Filename prefix → subdomain mapping for high-signal patterns.
function tagsFromFilename(basename: string): Partial<DocTags> | null {
  const bn = basename.toLowerCase();

  // ADR-NNN-* → Chorus / Loom / loom-decisions
  if (/^adr-\d+/.test(bn)) {
    return { product: 'chorus', subproduct: 'loom', subdomain: 'loom-decisions',
             confidence: 'high', signal: 'filename' };
  }
  // DEC-NNN-* → Chorus / Loom / loom-decisions
  if (/^dec-\d+/.test(bn)) {
    return { product: 'chorus', subproduct: 'loom', subdomain: 'loom-decisions',
             confidence: 'high', signal: 'filename' };
  }
  // domain-X.html → Gathering, X-domain (X is the subdomain shortname)
  const dm = bn.match(/^domain-([a-z]+)\.html?$/);
  if (dm) {
    const sub = `${dm[1]}-domain`;
    if (GATHERING_SUBDOMAINS.has(sub)) {
      return { product: 'gathering', subdomain: sub, confidence: 'high', signal: 'filename' };
    }
  }
  // service-design-X.html → tag by X if X matches a known subdomain
  const sd = bn.match(/^service-design-([a-z-]+)\.html?$/);
  if (sd) {
    const candidate = sd[1].endsWith('-domain') ? sd[1] : `${sd[1]}-domain`;
    const sp = SUBDOMAIN_TO_SUBPRODUCT[candidate];
    if (sp) {
      return { product: 'chorus', subproduct: sp, subdomain: candidate,
               confidence: 'high', signal: 'filename' };
    }
  }
  // Subproduct keyword in name → Chorus / <subproduct>
  // Borg demoted to Chorus subproduct per ontology change 2026-04-28.
  for (const sp of ['loom', 'werk', 'athena', 'convergence', 'clearing', 'borg']) {
    if (bn.startsWith(`${sp}-`) || bn.includes(`-${sp}-`) || bn.includes(`${sp}_`)) {
      return { product: 'chorus', subproduct: sp,
               confidence: 'medium', signal: 'filename' };
    }
  }
  // UPPERCASE_TOKEN substring matches (CHORUS_*, ATTENTION_*, BORG_*, etc.)
  // catch the data/about/<TOKEN>.html naming style
  if (/(chorus|borg|wren|silas|kade|werk|loom|athena|convergence|attention|memory|spine|nudge|clearing)/i.test(bn)) {
    return { product: 'chorus', confidence: 'medium', signal: 'filename' };
  }
  if (/(gathering|garden|photo|blog|wordpress)/i.test(bn)) {
    return { product: 'gathering', confidence: 'medium', signal: 'filename' };
  }
  return null;
}

// Path → tags (highest confidence — directory structure is canonical).
// Matches both filesystem-style paths ('public/gathering-docs/foo')
// and catalog-source-style labels ('gathering-docs/foo').
// eslint-disable-next-line complexity
function tagsFromPath(sourcePath: string): Partial<DocTags> | null {
  const p = sourcePath.toLowerCase();

  if (p.includes('/adr/') || p.startsWith('adr/') ||
      p.startsWith('roles/silas/adr') || p.startsWith('architect/adr')) {
    return { product: 'chorus', subproduct: 'loom', subdomain: 'loom-decisions',
             confidence: 'high', signal: 'path' };
  }
  if (p.includes('akasha/') || p.startsWith('akasha')) {
    // Akasha is one strand of the Consulting product line (2026-04-28 ontology shift).
    return { product: 'consulting', confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('gathering-docs') || p.startsWith('public/gathering-docs')) {
    return { product: 'gathering', confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('chorus-docs') || p.startsWith('public/chorus-docs')) {
    return { product: 'chorus', confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('designing/decisions') || p.startsWith('wren/decisions')) {
    return { product: 'chorus', subproduct: 'loom', subdomain: 'loom-decisions',
             confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('designing/docs')) {
    return { product: 'chorus', confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('roles/wren') || p.startsWith('roles/silas') || p.startsWith('roles/kade') ||
      p.startsWith('wren/') || p.startsWith('architect/')) {
    return { product: 'chorus', confidence: 'high', signal: 'path' };
  }
  if (p.startsWith('data/about') || p.startsWith('docs/diagrams') || p.startsWith('docs/')) {
    // data/about + docs/diagrams + docs/ are all chorus-content cabinets per #2510
    return { product: 'chorus', confidence: 'high', signal: 'path' };
  }
  // manual/<loom-path> hrefs in registry point at chorus-api's /loom/*
  if (p.startsWith('manual/loom/') || p.includes('/loom/')) {
    return { product: 'chorus', subproduct: 'loom', confidence: 'high', signal: 'path' };
  }
  // public/<root-file> defaults to gathering — these are the gathering app's
  // root-served HTML pages (business-plan, value-stream-render, gemba-analysis,
  // etc.). Some are cross-product narratives but most live in gathering.
  if (p.startsWith('public/') || p.match(/^[a-z]/)) {
    // Last-resort: source label 'public' without further specificity
    if (p.startsWith('public/')) {
      return { product: 'gathering', confidence: 'medium', signal: 'path' };
    }
  }
  return null;
}

// Content keyword scan — last-resort signal.
function tagsFromContent(contentHead: string | undefined): Partial<DocTags> | null {
  if (!contentHead) return null;
  const head = contentHead.toLowerCase();
  const chorusScore = head.split(/\b(chorus|borg|wren|silas|kade|werk|loom|athena)\b/).length - 1;
  const gatheringScore = head.split(/\b(gathering|garden|blog|photo|wordpress)\b/).length - 1;
  if (chorusScore >= 3 && chorusScore >= 2 * gatheringScore) {
    return { product: 'chorus', confidence: 'low', signal: 'content' };
  }
  if (gatheringScore >= 3 && gatheringScore >= 2 * chorusScore) {
    return { product: 'gathering', confidence: 'low', signal: 'content' };
  }
  return null;
}

// eslint-disable-next-line complexity
export function inferTags(input: DocTagInput): DocTags {
  // 1. Frontmatter override wins
  const fm = input.frontmatter;
  if (fm && (fm.product || fm.subproduct || fm.subdomain)) {
    return {
      product: fm.product,
      subproduct: fm.subproduct,
      subdomain: fm.subdomain,
      confidence: 'high',
      signal: 'frontmatter',
    };
  }

  // 2. Path is canonical when present — its signal beats filename's even
  // if both match the same thing. Filename enriches missing fields.
  const fromFilename = tagsFromFilename(input.basename);
  const fromPath = tagsFromPath(input.sourcePath);

  let chosen: Partial<DocTags> | null = fromPath || fromFilename;

  // Enrich chosen with missing fields from the other source
  if (chosen && chosen === fromPath && fromFilename) {
    if (!chosen.subproduct && fromFilename.subproduct) chosen.subproduct = fromFilename.subproduct;
    if (!chosen.subdomain && fromFilename.subdomain) chosen.subdomain = fromFilename.subdomain;
  }
  // er-diagram / ontology in chorus → Athena subproduct
  if (chosen?.product === 'chorus' && !chosen.subproduct) {
    const bn = input.basename.toLowerCase();
    if (/er-diagram|ontology|owl|class-diagram|instance-explorer|data-model/.test(bn)) {
      chosen.subproduct = 'athena';
    }
  }

  // 3. Last resort: content
  if (!chosen) {
    const c = tagsFromContent(input.contentHead);
    if (c) chosen = c;
  }

  if (!chosen) {
    return { confidence: 'none', signal: 'none' };
  }

  // Backfill subproduct from subdomain if known
  if (chosen.subdomain && !chosen.subproduct && SUBDOMAIN_TO_SUBPRODUCT[chosen.subdomain]) {
    chosen.subproduct = SUBDOMAIN_TO_SUBPRODUCT[chosen.subdomain];
  }

  return {
    product: chosen.product,
    subproduct: chosen.subproduct,
    subdomain: chosen.subdomain,
    confidence: chosen.confidence ?? 'none',
    signal: chosen.signal ?? 'none',
  };
}
