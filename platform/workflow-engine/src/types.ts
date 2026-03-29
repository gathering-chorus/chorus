// Workflow manifest types — matches existing JSON schema from workflow.sh

export type WorkflowStatus = 'pending' | 'in_progress' | 'completed' | 'blocked' | 'cancelled';
export type StepStatus = 'pending' | 'ready' | 'in_progress' | 'completed' | 'blocked' | 'skipped';
export type Role = 'silas' | 'kade' | 'wren' | 'jeff';

export interface Step {
  seq: number;
  role: Role;
  action: string;
  status: StepStatus;
  card: number | null;
  blocked_by: number[];
  artifacts: string[];
  brief: string | null;
  started_at: string | null;
  completed_at: string | null;
  notes: string | null;
}

export interface HistoryEvent {
  timestamp: string;
  event: string;
  role: string;
  detail: string;
}

export interface WorkflowManifest {
  id: string;
  decision: string;
  source: string;
  card: number | null;
  created: string;
  updated: string;
  status: WorkflowStatus;
  steps: Step[];
  verification: unknown;
  history: HistoryEvent[];
}

export interface AdvanceResult {
  completedStep: Step;
  nextStep: Step | null;
  workflowCompleted: boolean;
  briefPath: string | null;
  manifest: WorkflowManifest;
}

export interface PendingStep {
  workflowId: string;
  decision: string;
  step: Step;
  card: number | null;
}

export interface HandoffEvent {
  id: string;
  type: string;
  from: string;
  to: string;
  artifact: string;
  status: 'sent' | 'received' | 'stale';
  timestamp: string;
  workflow?: string;
  step?: number;
  received_by?: string;
}

export interface WorkflowEngineConfig {
  activeDir: string;
  archiveDir: string;
  briefDirs: Record<string, string>;
  handoffLogPath: string;
}
