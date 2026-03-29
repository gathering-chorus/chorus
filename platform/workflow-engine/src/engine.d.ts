import { WorkflowManifest, HistoryEvent, AdvanceResult, PendingStep, WorkflowEngineConfig } from './types';
export declare class WorkflowEngine {
    private config;
    constructor(config?: Partial<WorkflowEngineConfig>);
    nextId(): string;
    private manifestPath;
    private archivePath;
    load(wfId: string): WorkflowManifest;
    private save;
    private archive;
    scanWorkflows(includeArchive?: boolean): WorkflowManifest[];
    create(decision: string, stepsStr: string, source?: string, card?: number): WorkflowManifest;
    advance(wfId: string, notes?: string, artifacts?: string): AdvanceResult;
    private retireBriefs;
    private logHandoff;
    status(wfId?: string): WorkflowManifest | WorkflowManifest[];
    list(all?: boolean): WorkflowManifest[];
    pending(role: string): PendingStep[];
    history(wfId: string): HistoryEvent[];
}
