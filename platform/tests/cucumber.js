// #2614: gate @e2e scenarios behind RUN_INTEGRATION. They write to real
// /tmp/chorus-chat/ + invoke real chat.sh + race live role sessions. Default
// `npm test` skips them; explicit `RUN_INTEGRATION=1 npx cucumber-js` runs all.
// @wip tag also excluded by default per existing convention.
const runIntegration = !!process.env.RUN_INTEGRATION;

module.exports = {
  default: {
    requireModule: ['ts-node/register'],
    require: ['features/step_definitions/**/*.ts'],
    paths: ['features/**/*.feature'],
    format: ['progress-bar', 'summary'],
    publishQuiet: true,
    tags: runIntegration ? 'not @wip' : 'not @e2e and not @wip',
  },
};
