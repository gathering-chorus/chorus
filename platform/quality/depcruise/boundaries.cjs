// #4889 (Jeff 19:04, "fine do that"): dependency-cruiser in the architecture review.
// /gate-arch item 3 (boundaries): a package reaches another package through its door
// (its package entry), never into its files by a relative path. Item 1-ish: no cycles.
// The review fails on an `error` rule that a changed file breaks.
const PKGS = "(directing/clearing-v2|directing/clearing|directing/products/cards|platform/api|platform/chorus-sdk|platform/pulse|platform/workflow-engine|platform/agent-adapters)";
module.exports = {
  forbidden: [
    {
      name: "no-reach-into-another-package",
      severity: "error",
      comment: "/gate-arch item 3: import another package by its name (its door), not by a path into its files",
      from: { path: `^${PKGS}/` },
      // chorus-sdk is the shared library every package imports by path (14 of 14 hits on
      // 2026-11-04 were the sdk); the rule is about domain packages reaching into each other
      to: { path: `^${PKGS}/`, pathNot: ["^$1/", "^platform/chorus-sdk/"] },
    },
    {
      name: "no-circular",
      severity: "error",
      comment: "/gate-arch item 3: a cycle means two modules own one thing",
      from: {},
      to: { circular: true },
    },
  ],
  options: {
    doNotFollow: { path: "node_modules" },
    exclude: { path: "(node_modules|dist|build|coverage|/tests?/fixtures/)" },
    tsPreCompilationDeps: true,
  },
};
