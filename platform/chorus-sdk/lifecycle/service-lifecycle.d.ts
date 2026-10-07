// #4446 — types for service-lifecycle.js.
export interface LastExit { exitCode: number | null; signal: string | null }
export type LifecycleEvent = [string, Record<string, string>];
export function parseLastExit(print: string): LastExit | null;
export function startEvents(service: string, pid: number, version: string, previous: LastExit | null): LifecycleEvent[];
export function stopEvent(service: string, pid: number, reason: string): LifecycleEvent;
export function failedEvent(service: string, pid: number, reason: string, exitCode: number): LifecycleEvent;
export function exitEvent(service: string, pid: number, code: number, ended: boolean): LifecycleEvent | null;
export function launchdLabel(): string | null;
export function previousRun(label: string): LastExit | null;
export function scriptVersion(file?: string): string;
export interface ServiceLifecycle {
  service: string;
  started(version?: string): void;
  stopped(reason: string): void;
  failed(reason: string, exitCode?: number): void;
  /** Refuse to run: the reason on stderr, service.failed with it, then exit. */
  refuse(reason: string, exitCode?: number): never;
}
export function serviceLifecycle(name: string): ServiceLifecycle;
