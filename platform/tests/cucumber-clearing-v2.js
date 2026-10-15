module.exports = {
  default: {
    requireModule: ['ts-node/register'],
    require: ['features/step_definitions/clearing_v2_steps.ts'],
    paths: ['features/clearing-v2/*.feature'],
    format: ['summary'],
    publishQuiet: true,
  },
};
