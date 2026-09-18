// @test-type: unit — fixture-data, not a suite: this file exists to be READ by the #4201 placement proofs, never run.
// #4201 — fixture, not a suite. Two rules disagree about this file on purpose:
// the route it calls says `cards`, the card in its header says `search`.
// The pass must refuse to tag it and print one conflict line naming both.
import request from 'supertest';

describe('a file two rules read differently', () => {
  it('calls the cards route', async () => {
    await request(app).get('/api/chorus/cards');
  });
});
