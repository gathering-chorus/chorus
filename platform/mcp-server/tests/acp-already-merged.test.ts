// #3040 — acp recovery after squash-merge. When `gh pr create` fails with
// "No commits between <base> and <head>", the work is already on main (the
// squash merged it); acp must recover (skip pr-merge, finish werk-close +
// cards-done) instead of refusing pr-create-fail. This pins the detection
// predicate that routes the catch to recovery vs refusal.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { prCreateMeansAlreadyMerged } from '../src/server';

test('#3040 detects the real squash-merge gh error (kade/3038, wren/3025)', () => {
  assert.equal(prCreateMeansAlreadyMerged('pull request create failed: GraphQL: No commits between main and kade/3038 (createPullRequest)'), true);
  assert.equal(prCreateMeansAlreadyMerged('GraphQL: No commits between main and wren/3025 (createPullRequest)'), true);
});

test('#3040 case-insensitive + spacing tolerant', () => {
  assert.equal(prCreateMeansAlreadyMerged('no commits between main and silas/2967'), true);
});

test('#3040 does NOT swallow genuine pr-create failures', () => {
  assert.equal(prCreateMeansAlreadyMerged('GraphQL: a pull request already exists for kade:kade/3040'), false);
  assert.equal(prCreateMeansAlreadyMerged('could not compute title for pull request'), false);
  assert.equal(prCreateMeansAlreadyMerged('HTTP 403: Resource not accessible by integration'), false);
  assert.equal(prCreateMeansAlreadyMerged(''), false);
});
