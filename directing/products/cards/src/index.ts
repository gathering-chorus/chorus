// API layer
export { BoardClient } from './client';

// Config
export { GATHERING, SELF, LABELS, loadEnv, detectRole, resolveBucket } from './config';

// Types
export type { BoardConfig, BoardTask, BoardSnapshot, VikunjaTask, Role, Priority } from './types';

// Events
export { emitSpineEvent, emitChorusEvent } from './events';
export type { SpineEvent } from './events';

// SDK — high-level operations (importable by scripts)
export {
  addCard, moveCard, doneCard, demoCard, rejectCard,
  blockCard, unblockCard, updateCard, commentCard, tagCard, untagCard, setCard,
  swatCard, snapshotBoard, auditStart, auditClose,
  triggerWorkflow, reconcileWorkflows,
  notifyOwnerIfDifferent, notifyPM,
} from './sdk';
