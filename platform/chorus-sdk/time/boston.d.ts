// #4885 — types for boston.js, the one TS/JS Boston renderer.
export function bostonOffset(d?: Date): string;
export function isBostonDaylight(d?: Date): boolean;
export function bostonOffsetIso(d?: Date): string;
export function bostonLocal(d?: Date): string;
export function bostonMinute(d?: Date): string;
export function bostonDay(d?: Date): string;
export function bostonHour(d?: Date): number;
export function bostonHHMM(d?: Date): string;
export function bostonMidnight(d?: Date): number;
export function bostonFormat(d: Date, opts: Intl.DateTimeFormatOptions, locale?: string): string;
