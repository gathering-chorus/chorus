// @domain: messages
// #4887 — the one TS/JS reply hash, the join key between chorus-hooks' reply.emitted
// (Rust content_hash, reply_delivery.rs) and a reply's row in the room. The contract
// is config/reply-hash-fixtures.json: strip one LEADING '--- ... ---' chorus header,
// collapse whitespace runs to one space, trim; sha256 of the utf8, hex, first 16.
// Nothing beyond that — each extra normalization is a place for the two to disagree.
// Moved here from Clearing v1's reply-delivery.ts when its reply.rendered echo retired.
'use strict';

const { createHash } = require('crypto');

function canonicalize(text) {
  const stripped = text.trimStart().replace(/^---[^]*?---\s*/, '');
  return stripped.split(/\s+/).filter(Boolean).join(' ');
}

/** Join key for one reply across surfaces. sha256(canonicalize(text)), hex[..16]. */
function contentHash(text) {
  return createHash('sha256').update(canonicalize(text), 'utf8').digest('hex').slice(0, 16);
}

module.exports = { contentHash };
