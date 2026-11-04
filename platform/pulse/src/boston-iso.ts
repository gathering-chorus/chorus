/**
 * boston-iso.ts — #3880: one clock on the spine; #4885: one renderer.
 *
 * The formatter lives in platform/chorus-sdk/time/boston (shared by pulse, the api and Clearing);
 * this module only re-exports it so pulse's imports stay put. Never reimplement it here.
 */
export { bostonOffsetIso, bostonMinute } from '../../chorus-sdk/time/boston';
