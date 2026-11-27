import { request } from './api.ts';
import type { SpecRevision } from './pages/SpecsDetail';

/**
 * Spec and plan endpoints for the Specs page. Every call goes through the
 * shared {@link request} helper, so an HTTP error or a non-JSON reply (a
 * proxy error page, an expired share token) is an `ApiError` with the
 * status and the server's message.
 */

/** The raw spec list; `parseSpecPayload` turns it into hierarchy or legacy rows. */
export type SpecListReply = Record<string, unknown> & { error?: string };

export interface RevisionsReply {
  revisions?: SpecRevision[];
  error?: string;
}

/** A write reply: `ok` false carries the server's reason in `error`. */
export interface SpecWriteReply {
  ok?: boolean;
  error?: string;
  version?: number;
  planId?: string;
  planVersion?: number;
}

const enc = encodeURIComponent;

export function fetchSpecList(): Promise<SpecListReply> {
  return request<SpecListReply>('GET', '/specs');
}

export function fetchSpecRevisions(specId: string): Promise<RevisionsReply> {
  return request<RevisionsReply>('GET', `/specs/${enc(specId)}/revisions`);
}

export function fetchPlanRevisions(planId: string): Promise<RevisionsReply> {
  return request<RevisionsReply>('GET', `/specs/plan/${enc(planId)}/revisions`);
}

export function approveSpec(specId: string, version: number): Promise<SpecWriteReply> {
  return request<SpecWriteReply>('POST', `/specs/${enc(specId)}/approve`, { version });
}

export function saveSpecRevision(specId: string, basedOn: number, body: string): Promise<SpecWriteReply> {
  return request<SpecWriteReply>('POST', `/specs/${enc(specId)}/revisions`, { basedOn, body });
}

export function saveSpecPlanRevision(input: {
  planId: string;
  basedOn: number;
  body: string;
  sessionId: string;
  turnId: string;
}): Promise<SpecWriteReply> {
  return request<SpecWriteReply>('POST', '/specs/plan-revision', input);
}

export function createAlternativePlan(input: {
  projectId: string;
  specId: string;
  planId: string;
  planTitle: string;
  planBody: string;
  sessionId: string;
  turnId: string;
}): Promise<SpecWriteReply> {
  return request<SpecWriteReply>('POST', '/specs/alternative-plan', input);
}
