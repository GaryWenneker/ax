import type { Handler } from '@netlify/functions';
/** Prefer Accept: text/html (browsers / axe) → HTML with lang + title; else plain text for installers. */
export declare function createHandler(version: string): Handler;
export declare const handler: Handler;
