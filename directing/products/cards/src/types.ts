/** Vikunja related-task reference (subset of VikunjaTask used in relation arrays) */
export interface VikunjaRelatedTask {
  id: number;
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
  /** Relation edges — Vikunja groups by relation kind. Only `blocked` used today. */
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
