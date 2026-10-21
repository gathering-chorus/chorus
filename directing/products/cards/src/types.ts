/** Vikunja related-task reference (subset of VikunjaTask used in relation arrays) */
export interface VikunjaRelatedTask {
  id: number;
  /** Display index (#4669 — Vikunja returns the full related task) */
  index?: number;
  title?: string;
  done: boolean;
}

/** Vikunja task as returned by the API */
export interface VikunjaTask {
  id: number;
  index: number;
  title: string;
  description: string;
  done: boolean;
  created: string;
  updated: string;
  /** Vikunja returns `null` (not `[]`) for tasks with no labels. #2512 */
  labels: VikunjaLabel[] | null;
  project_id: number;
  /** Relation edges — Vikunja groups by relation kind: `blocked` sequences, parenttask/subtask is the child-card family (#4669). */
  related_tasks?: {
    blocked?: VikunjaRelatedTask[];
    blocking?: VikunjaRelatedTask[];
    subtask?: VikunjaRelatedTask[];
    parenttask?: VikunjaRelatedTask[];
  };
}

export interface VikunjaLabel {
  id: number;
  title: string;
}

export interface VikunjaBucket {
  id: number;
  title: string;
  limit: number;
  tasks: VikunjaTask[] | null;
}

/** Parsed task with resolved metadata */
export interface BoardTask {
  index: number;
  apiId: number;
  title: string;
  description: string;
  status: string;
  owner: string;
  priority: string;
  domains: string[];
  done: boolean;
  created: string;
  updated: string;
  /** #4669 — the card this one is a child of (set by view()) */
  parent?: number;
  /** #4669 — this card's children with their status (set by view()) */
  children?: CardChild[];
}

export interface CardChild {
  index: number;
  title: string;
  status: string;
}

export interface BoardConfig {
  name: string;
  projectId: number;
  viewId: number;
  buckets: Record<string, number>;
  /** Reverse map: bucket ID → display name */
  bucketNames: Record<number, string>;
}

export interface BoardSnapshot {
  board: string;
  timestamp: string;
  tasks: BoardTask[];
}

export type Role = 'wren' | 'silas' | 'kade' | 'jeff';
export type Priority = 'P1' | 'P2' | 'P3';
