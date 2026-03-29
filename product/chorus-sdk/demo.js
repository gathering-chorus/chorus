#!/usr/bin/env node
// Chorus SDK — live demo for Jeff (#973)
// Three capabilities: emit, search, subscribe

const { emit, search, subscribe } = require('./dist');

async function demo() {
  console.log('\n=== Chorus SDK Demo ===\n');

  // 1. EMIT — fire a real spine event
  console.log('1. EMIT — writing a spine event to chorus.log');
  const event = emit('sdk.demo.fired', 'silas', {
    card: '973',
    detail: 'live walkthrough for Jeff',
  });
  console.log('   Event:', event.event, '| Role:', event.role, '| Time:', event.timestamp);
  console.log('   ✓ Written to chorus.log\n');

  // 2. SEARCH — query the Chorus index for real data
  console.log('2. SEARCH — querying Chorus for "nudge"');
  try {
    const results = await search('nudge', 5);
    console.log(`   Found ${results.total} results. Top ${results.results.length}:`);
    for (const r of results.results.slice(0, 3)) {
      const preview = r.content.replace(/<[^>]+>/g, '').slice(0, 100);
      console.log(`   [${r.source}/${r.role}] ${preview}`);
    }
    console.log('   ✓ Chorus index is searchable\n');
  } catch (err) {
    console.log('   ✗ Search failed:', err.message, '\n');
  }

  // 3. SUBSCRIBE — watch for live events (5 second window)
  console.log('3. SUBSCRIBE — watching for spine events (5s window)');
  let seen = 0;
  const unsub = subscribe('sdk.demo', (evt) => {
    seen++;
    console.log(`   Live: ${evt.event} from ${evt.role}`);
  }, { pollInterval: 500 });

  // Fire a second event so subscribe catches it
  setTimeout(() => {
    emit('sdk.demo.ping', 'silas', { seq: '2' });
  }, 1500);

  setTimeout(() => {
    unsub();
    console.log(`   ✓ Caught ${seen} event(s) in 5s window\n`);
    console.log('=== Demo complete ===\n');
  }, 5000);
}

demo();
