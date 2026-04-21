// indexAllSources (extracted from server.ts for #2205 wave 18).
//
// Biggest single extraction of the card. 11 independent source indexers
// share a prepared-statement + transaction context, each wrapped in
// try/catch so one source's failure doesn't tank the others. Factory
// form with injected fs / path / dbPath / homedir keeps tests hermetic.

export interface IndexAllSourcesDeps {
  dbPath: string;
  DatabaseCtor: new (path: string) => {
    pragma: (s: string) => void;
    prepare: (sql: string) => {
      run?: (...args: any[]) => any;
      all?: (...args: any[]) => any[];
      get?: (...args: any[]) => any;
    };
    transaction: (fn: (events: any[]) => void) => (events: any[]) => void;
    close: () => void;
  };
  fs: {
    existsSync: (p: string) => boolean;
    readdirSync: (p: string) => string[];
    readFileSync: (p: string, enc: string) => any;
    statSync: (p: string) => { mtime: { toISOString: () => string } };
  };
  path: {
    join: (...parts: string[]) => string;
  };
  repoRoot: string;
  homedir: () => string;
  now?: () => string;
}

// eslint-disable-next-line max-lines-per-function -- #2288 pre-existing threshold violation, tracked for refactor
export function createIndexAllSources(deps: IndexAllSourcesDeps): () => Promise<{ indexed: Record<string, string>; elapsed_ms: number }> {
  const nowFn = deps.now ?? (() => new Date().toISOString());
  // eslint-disable-next-line complexity, max-lines-per-function -- #2288 pre-existing threshold violation, tracked for refactor
  return async function indexAllSources() {
    const db = new deps.DatabaseCtor(deps.dbPath);
    db.pragma('journal_mode = WAL');
    const results: Record<string, string> = {};
    const startTime = Date.now();

    const insert = db.prepare(`
      INSERT OR IGNORE INTO messages (source, source_id, channel, role, author, content, timestamp)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
    const updateWatermark = db.prepare(`
      INSERT INTO watermarks (source, last_seen, last_indexed) VALUES (?, ?, ?)
      ON CONFLICT(source) DO UPDATE SET last_seen = excluded.last_seen, last_indexed = excluded.last_indexed
    `);

    const now = nowFn();
    const { fs, path, repoRoot, homedir } = deps;

    // 1. Spine events
    try {
      const logPath = path.join(repoRoot, 'platform/logs/chorus.log');
      if (fs.existsSync(logPath)) {
        const content = String(fs.readFileSync(logPath, 'utf-8'));
        const lines = content.trim().split('\n');
        let indexed = 0;
        const insertMany = db.transaction((events: any[]) => {
          for (const e of events) {
            insert.run!(e.source, e.source_id, e.channel, e.role, e.author, e.content, e.timestamp);
            indexed++;
          }
        });
        const events: any[] = [];
        for (const line of lines) {
          try {
            const evt = JSON.parse(line);
            const role = evt.role || 'system';
            const event = evt.event || 'unknown';
            events.push({
              source: 'spine',
              source_id: `spine-${evt.timestamp}-${event}`,
              channel: `spine:${role}`,
              role,
              author: role,
              content: line,
              timestamp: evt.timestamp || now,
            });
          } catch { /* skip malformed */ }
        }
        insertMany(events);
        updateWatermark.run!('spine', now, now);
        results.spine = `${indexed} events indexed`;
      }
    } catch (err: any) { results.spine = `error: ${err.message}`; }

    // 2. Briefs
    try {
      let indexed = 0;
      for (const role of ['wren', 'silas', 'kade']) {
        const briefDir = path.join(repoRoot, `roles/${role}/briefs`);
        if (!fs.existsSync(briefDir)) continue;
        const files = fs.readdirSync(briefDir).filter(f => f.endsWith('.md') && !f.startsWith('.'));
        for (const file of files) {
          const filePath = path.join(briefDir, file);
          const stat = fs.statSync(filePath);
          const content = String(fs.readFileSync(filePath, 'utf-8'));
          const ts = stat.mtime.toISOString();
          insert.run!('brief', `brief:${role}:${file}`, `brief:${role}`, role, role, content, ts);
          indexed++;
        }
      }
      updateWatermark.run!('artifact:brief', now, now);
      results.briefs = `${indexed} briefs indexed`;
    } catch (err: any) { results.briefs = `error: ${err.message}`; }

    // 3. Decisions
    try {
      const decPath = path.join(repoRoot, 'roles/wren/decisions.md');
      if (fs.existsSync(decPath)) {
        const content = String(fs.readFileSync(decPath, 'utf-8'));
        const decisions = content.split('\n## DEC-').filter(Boolean);
        let indexed = 0;
        for (const dec of decisions) {
          const firstLine = dec.split('\n')[0];
          const id = firstLine.match(/^(\d+)/)?.[1] || 'unknown';
          insert.run!('decision', `decision:DEC-${id}`, 'decisions', 'wren', 'wren', `## DEC-${dec}`, now);
          indexed++;
        }
        updateWatermark.run!('artifact:decisions', now, now);
        results.decisions = `${indexed} decisions indexed`;
      }
    } catch (err: any) { results.decisions = `error: ${err.message}`; }

    // 4. ADRs
    try {
      const adrDir = path.join(repoRoot, 'roles/silas/adr');
      if (fs.existsSync(adrDir)) {
        const files = fs.readdirSync(adrDir).filter(f => f.endsWith('.md'));
        let indexed = 0;
        for (const file of files) {
          const content = String(fs.readFileSync(path.join(adrDir, file), 'utf-8'));
          insert.run!('adr', `adr:${file}`, 'adr:silas', 'silas', 'silas', content, now);
          indexed++;
        }
        updateWatermark.run!('artifact:adr', now, now);
        results.adrs = `${indexed} ADRs indexed`;
      }
    } catch (err: any) { results.adrs = `error: ${err.message}`; }

    // 5. Activity log
    try {
      const actPath = path.join(repoRoot, 'activity.md');
      if (fs.existsSync(actPath)) {
        const content = String(fs.readFileSync(actPath, 'utf-8'));
        insert.run!('activity', 'activity:latest', 'activity', 'system', 'system', content, now);
        updateWatermark.run!('artifact:activity', now, now);
        results.activity = 'indexed';
      }
    } catch (err: any) { results.activity = `error: ${err.message}`; }

    // 6. Memory files
    try {
      const memDir = path.join(homedir(), '.claude/projects');
      if (fs.existsSync(memDir)) {
        let indexed = 0;
        const dirs = fs.readdirSync(memDir).filter(d => d.includes('chorus'));
        for (const dir of dirs) {
          const memoryDir = path.join(memDir, dir, 'memory');
          if (!fs.existsSync(memoryDir)) continue;
          const files = fs.readdirSync(memoryDir).filter(f => f.endsWith('.md'));
          for (const file of files) {
            const content = String(fs.readFileSync(path.join(memoryDir, file), 'utf-8'));
            insert.run!('memory', `memory:${file}`, 'memory', 'system', 'system', content, now);
            indexed++;
          }
        }
        updateWatermark.run!('artifact:memory', now, now);
        results.memory = `${indexed} memory files indexed`;
      }
    } catch (err: any) { results.memory = `error: ${err.message}`; }

    // 7. State files
    try {
      let indexed = 0;
      for (const role of ['wren', 'silas', 'kade']) {
        const nsPath = path.join(repoRoot, `roles/${role}/next-session.md`);
        if (fs.existsSync(nsPath)) {
          const content = String(fs.readFileSync(nsPath, 'utf-8'));
          insert.run!('state', `state:${role}:next-session`, `state:${role}`, role, role, content, now);
          indexed++;
        }
      }
      updateWatermark.run!('artifact:state', now, now);
      results.state = `${indexed} state files indexed`;
    } catch (err: any) { results.state = `error: ${err.message}`; }

    // 8. Clearing transcripts
    try {
      const chatDir = '/tmp/chorus-chat';
      if (fs.existsSync(chatDir)) {
        let indexed = 0;
        const files = fs.readdirSync(chatDir).filter(f => f.endsWith('.md'));
        for (const file of files) {
          const content = String(fs.readFileSync(path.join(chatDir, file), 'utf-8'));
          const stat = fs.statSync(path.join(chatDir, file));
          insert.run!('clearing', `clearing:${file}`, 'clearing:session', 'system', 'system', content, stat.mtime.toISOString());
          indexed++;
        }
        updateWatermark.run!('clearing', now, now);
        results.clearing = `${indexed} transcripts indexed`;
      }
    } catch (err: any) { results.clearing = `error: ${err.message}`; }

    // 9. Journal entries
    try {
      let indexed = 0;
      for (const role of ['wren', 'silas', 'kade']) {
        const journalDir = path.join(repoRoot, `roles/${role}/journal`);
        if (!fs.existsSync(journalDir)) continue;
        const files = fs.readdirSync(journalDir).filter(f => f.endsWith('.md'));
        for (const file of files) {
          const content = String(fs.readFileSync(path.join(journalDir, file), 'utf-8'));
          insert.run!('journal', `journal:${role}:${file}`, `journal:${role}`, role, role, content, now);
          indexed++;
        }
      }
      updateWatermark.run!('journal', now, now);
      results.journal = `${indexed} journal entries indexed`;
    } catch (err: any) { results.journal = `error: ${err.message}`; }

    // 10. Stories
    try {
      let indexed = 0;
      const storiesFile = path.join(repoRoot, 'roles/wren/self-stories.md');
      if (fs.existsSync(storiesFile)) {
        const content = String(fs.readFileSync(storiesFile, 'utf-8'));
        const stories = content.split('\n## ').filter(Boolean);
        for (const story of stories) {
          const title = story.split('\n')[0].trim();
          const id = title.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase().slice(0, 50);
          insert.run!('story', `story:${id}`, 'stories', 'wren', 'jeff', `## ${story}`, now);
          indexed++;
        }
      }
      const archiveDir = path.join(repoRoot, 'roles/wren/briefs-archive');
      if (fs.existsSync(archiveDir)) {
        const storyFiles = fs.readdirSync(archiveDir).filter(f => f.includes('story'));
        for (const file of storyFiles) {
          const content = String(fs.readFileSync(path.join(archiveDir, file), 'utf-8'));
          insert.run!('story', `story:brief:${file}`, 'stories', 'wren', 'jeff', content, now);
          indexed++;
        }
      }
      updateWatermark.run!('stories', now, now);
      results.stories = `${indexed} stories indexed`;
    } catch (err: any) { results.stories = `error: ${err.message}`; }

    // 11. Slack — deprecated
    try {
      db.prepare('DELETE FROM watermarks WHERE source LIKE \'slack%\'').run!();
      db.prepare('DELETE FROM watermarks WHERE source = \'slack\'').run!();
      results.slack = 'removed (deprecated)';
    } catch { /* ignore */ }

    db.close();

    return {
      indexed: results,
      elapsed_ms: Date.now() - startTime,
    };
  };
}
