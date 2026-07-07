// Parse set folder name into { name, category, site }
// Pattern: name(s)-category-site-id
// e.g. "sabrina-banks-solo-f-nubiles-porn-1_cutie-pie_1200"
//   → { name: "sabrina-banks", category: "solo-f", site: "nubiles-porn" }

const CATEGORIES = [
  'pov-blowjob-fm', 'pov-mm', 'solo-f', 'solo-m', 'solo-t',
  'joi-f', 'ffm', 'fmm', 'fff', 'fm', 'ff', 'mm', 'tf', 'tm'
];

// Canonical studios — sorted longest-first for greedy matching
// Source: sexuality.handler.ts STUDIOS list
const STUDIOS = [
  'digital-playground', 'reality-kings', 'lucas-entertainment', 'corbin-fisher',
  'naughty-america', 'naughtyamerica', 'sweetheart-video', 'girlfriends-films',
  'modern-day-sins', 'raging-stallion', 'manuel-ferrara', 'elegant-angel',
  'burning-angel', 'holly-randall', 'digital-desire', 'harmony-films',
  'devils-films', 'cherry-pimps', 'evil-angel', 'jules-jordan', 'sweet-sinner',
  'pure-taboo', 'puretaboo', 'watch4beauty', 'sean-cody', 'adult-time',
  'adulttime', 'web-young', 'webyoung', 'icon-male', 'tushy-raw', 'blacked-raw',
  '1111customs', 'castingcouchx', 'realitykings', 'nubilefilms', 'teamskeet',
  'penthouse', 'pansexual', 'passionhd', 'brazzers', 'girlsway', 'ftvgirls',
  'cockyboys', 'archangel', 'bangbros', 'nubiles-porn', 'nubiles-net', 'nubiles',
  'club-sweethearts', 'kristen-bjorn', 'trans-angels', 'all-girl-massage',
  'new-sensations', 'twistys', 'bellesa', 'blacked', 'deeper', 'falcon',
  'belami', 'dorcel', 'sexart', 'spizoo', 'slayed', 'wicked', 'vixen', 'tushy',
  'mofos', 'helix', 'hardx', 'darkx', 'tiny4k', 'lubed', 'povd', 'bang',
  'babes', 'x-art', 'suze', 'hegre', 'femjoy', 'met-art', 'joymii',
  'als-scan', 'ftv-girls', 'zishy', 'wow-girls', 'lovehairy',
  'anal-teen-angels', 'anal-teen-club', 'bare-maidens', 'amateur-gay-pov',
  'asshole-fever', 'anilos', 'anal-thrills', 'adam-and-eve',
  'pinko-tgirls', 'active-duty', 'gay-room', 'next-door',
  'grooby', 'gender-x', 'men', 'diabolic', 'nuru-massage', 'fantasy-massage',
  'combat-zone', 'mr-lucky-pov', 'true-amateurs', 'biphoria', 'noir-male',
  'taboo-heat', 'asmr-fantasy', 'rickys-room', 'kiss-me-fuck-me',
  'bellesa-films', 'hotcrazymess', 'girlcum', 'swallowed',
];

export function parseSetFolder(folderName) {
  for (const cat of CATEGORIES) {
    const idx = folderName.indexOf(`-${cat}-`);
    if (idx === -1) continue;

    const name = folderName.substring(0, idx);
    const afterCat = folderName.substring(idx + cat.length + 2);

    // Try canonical studio match first (greedy — longest match wins)
    let site = 'unknown';
    for (const studio of STUDIOS) {
      if (afterCat.startsWith(studio + '-') || afterCat.startsWith(studio + '_') ||
          afterCat === studio || afterCat.startsWith(studio + '.')) {
        site = studio;
        break;
      }
    }

    // No fallback — canonical studio list is authoritative

    return { name, category: cat, site };
  }

  // No category found — try direct studio match in the full name
  for (const studio of STUDIOS) {
    const studioPattern = `-${studio}-`;
    const idx = folderName.indexOf(studioPattern);
    if (idx !== -1) {
      return { name: folderName.substring(0, idx), category: 'unknown', site: studio };
    }
    // Also check at end of string
    if (folderName.endsWith(`-${studio}`)) {
      return { name: folderName.substring(0, folderName.length - studio.length - 1), category: 'unknown', site: studio };
    }
  }

  return { name: folderName, category: 'unknown', site: 'unknown' };
}

// Known sites for video filename parsing (no category tag in video names)
const KNOWN_SITES = [
  'brazzers', 'reality-kings', 'teamskeet', 'bangbros', 'naughty-america',
  'blacked', 'tushy', 'vixen', 'deeper', 'blacked-raw', 'tushy-raw',
  'mofos', 'digital-playground', 'babes', 'twistys', 'nubiles-porn',
  'nubiles-net', 'passion-hd', 'passionhd', 'tiny4k', 'fantasyhd',
  'puremature', 'holed', 'lubed', 'exotic4k', 'pornpros',
  'evil-angel', 'jules-jordan', 'hard-x', 'darkx', 'eroticax',
  'sweet-sinner', 'new-sensations', 'wicked', 'elegant-angel',
  'penthouse', 'playboy', 'hustler', 'kink', 'falcon',
  'men', 'sean-cody', 'corbin-fisher', 'bel-ami', 'helix-studios',
  'cockyboys', 'kristen-bjorn', 'lucas-entertainment', 'raging-stallion',
  'icon-male', 'next-door', 'active-duty', 'gay-room',
  'trans-angels', 'grooby', 'gender-x',
  'hegre', 'met-art', 'femjoy', 'watch4beauty', 'x-art',
  'digital-desire', 'holly-randall', 'suze', 'club-sweethearts',
  'burning-angel', 'joymii', 'wow-girls', 'sexart', 'als-scan',
  'ftv-girls', 'zishy', 'this-years-model', 'amour-angels',
  'girlsway', 'webyoung', 'mommys-girl', 'all-girl-massage',
  'girlfriends-films',
];

export function parseVideoFilename(filename) {
  const lower = filename.toLowerCase();
  for (const site of KNOWN_SITES) {
    if (lower.includes(`-${site}-`) || lower.includes(`-${site}_`)) {
      return { site };
    }
  }
  // Try without hyphens — some filenames use concatenated site names
  for (const site of KNOWN_SITES) {
    const nohyphen = site.replace(/-/g, '');
    if (lower.includes(nohyphen)) {
      return { site };
    }
  }
  return { site: 'unknown' };
}

export function getTagFromParent(parentPath) {
  if (parentPath.includes('💄')) return '💄';
  if (parentPath.includes('🔴')) return '🔴';
  if (parentPath.includes('🟠')) return '🟠';
  if (parentPath.includes('🟢')) return '🟢';
  if (parentPath.includes('🟣')) return '🟣';
  return 'unknown';
}

