// #3682 — a test brings its own world (#3528). In the werk pipeline jest
// inherits the runner's env: NODE_ENV is pre-set (so jest does NOT set it to
// 'test') and DEPLOY_ROLE=<role> is exported. That combination flipped
// addCard's bouncer LIVE inside unit tests — process.exit(1) crashed two
// suites' workers and wrote real asks Jeff had to clear (observed 2026-08-23,
// runs -10/-21; since #4505 a live bouncer would post real Approval rows).
// Pin the world here, before any module loads.
process.env.NODE_ENV = 'test';
delete process.env.DEPLOY_ROLE;
delete process.env.CHORUS_ORIGIN_PRINCIPAL;
// #3102 — a unit test must never write the live graph. Tests that exercise the
// sync inject a fake door instead.
process.env.CARDS_GRAPH_SYNC = 'off';
