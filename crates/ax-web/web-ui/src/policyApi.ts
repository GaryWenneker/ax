import type {
  CaptureProposeResult,
  CaptureSaveResult,
  MatchResult,
  PolicyRuleDoc,
  PolicyRuleRow,
  PolicySkillDoc,
  PolicySkillRow,
  RuleFrontmatter,
  SkillFrontmatter,
} from './policyTypes';

const POLICY = '/api/policy';

interface ApiErrorBody {
  error?: string;
  fields?: Record<string, string>;
}

function formatApiError(body: ApiErrorBody, status: number): string {
  if (body.fields && Object.keys(body.fields).length > 0) {
    const details = Object.entries(body.fields)
      .map(([field, msg]) => `${field}: ${msg}`)
      .join('; ');
    return `${body.error ?? 'Request failed'} (${details})`;
  }
  return body.error ?? `HTTP ${status}`;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${POLICY}${path}`, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
    ...init,
  });
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as ApiErrorBody;
    throw new Error(formatApiError(body, res.status));
  }
  return res.json() as Promise<T>;
}

export function fetchPolicyRules(): Promise<{ rules: PolicyRuleRow[]; groups?: unknown }> {
  return request('/rules');
}

export function fetchPolicyRule(id: string): Promise<PolicyRuleDoc> {
  return request(`/rules/${encodeURIComponent(id)}`);
}

export function savePolicyRule(id: string | null, frontmatter: RuleFrontmatter, body: string): Promise<PolicyRuleDoc> {
  const payload = { frontmatter, body };
  if (id) {
    return request(`/rules/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(payload) });
  }
  return request('/rules', { method: 'POST', body: JSON.stringify(payload) });
}

export function deletePolicyRule(id: string): Promise<{ ok: boolean }> {
  return request(`/rules/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export function fetchPolicySkills(): Promise<{ skills: PolicySkillRow[]; groups?: Array<{ id: string; label: string; order: number }> }> {
  return request('/skills');
}

export function fetchPolicySkill(name: string): Promise<PolicySkillDoc> {
  return request(`/skills/${encodeURIComponent(name)}`);
}

export function savePolicySkill(
  name: string | null,
  frontmatter: SkillFrontmatter,
  body: string,
): Promise<PolicySkillDoc> {
  const payload = { frontmatter, body };
  if (name) {
    return request(`/skills/${encodeURIComponent(name)}`, { method: 'PUT', body: JSON.stringify(payload) });
  }
  return request('/skills', { method: 'POST', body: JSON.stringify(payload) });
}

export function deletePolicySkill(name: string): Promise<{ ok: boolean }> {
  return request(`/skills/${encodeURIComponent(name)}`, { method: 'DELETE' });
}

export function setPolicyRuleEnabled(id: string, enabled: boolean): Promise<{ ok: boolean }> {
  return request(`/rules/${encodeURIComponent(id)}/enabled`, {
    method: 'PATCH',
    body: JSON.stringify({ enabled }),
  });
}

export function setPolicySkillEnabled(name: string, enabled: boolean): Promise<{ ok: boolean }> {
  return request(`/skills/${encodeURIComponent(name)}/enabled`, {
    method: 'PATCH',
    body: JSON.stringify({ enabled }),
  });
}

export function setPolicyRuleStorage(
  id: string,
  storage: 'files' | 'database',
  keepFile = false,
): Promise<{ ok: boolean; effectiveStorage?: string; storageIsOverride?: boolean }> {
  return request(`/rules/${encodeURIComponent(id)}/storage`, {
    method: 'PATCH',
    body: JSON.stringify({ storage, keepFile }),
  });
}

export function setPolicySkillStorage(
  name: string,
  storage: 'files' | 'database',
  keepFile = false,
): Promise<{ ok: boolean; effectiveStorage?: string; storageIsOverride?: boolean }> {
  return request(`/skills/${encodeURIComponent(name)}/storage`, {
    method: 'PATCH',
    body: JSON.stringify({ storage, keepFile }),
  });
}

export function fetchPolicySyncSettings(): Promise<import('./policyTypes').PolicySyncSettings> {
  return request('/settings');
}

export function savePolicySyncSettings(
  patch: Partial<import('./policyTypes').PolicySyncSettings>,
): Promise<import('./policyTypes').PolicySyncSettings> {
  return request('/settings', { method: 'PUT', body: JSON.stringify(patch) });
}

export function fetchPolicyReview(): Promise<{ items: import('./policyTypes').PendingPolicyItem[] }> {
  return request('/review');
}

export function approvePolicyReview(id: string): Promise<{ ok: boolean }> {
  return request(`/review/${encodeURIComponent(id)}/approve`, { method: 'POST', body: '{}' });
}

export function rejectPolicyReview(id: string): Promise<{ ok: boolean }> {
  return request(`/review/${encodeURIComponent(id)}/reject`, { method: 'POST', body: '{}' });
}

export function fetchPolicyPackStatus(): Promise<import('./policyTypes').PolicyPackStatus> {
  return request('/pack/status');
}

export function exportPolicyPack(): Promise<{ rulesExported: number; skillsExported: number; path: string }> {
  return request('/pack/export', { method: 'POST', body: '{}' });
}

export function importPolicyPack(force = false): Promise<Record<string, number>> {
  return request('/pack/import', { method: 'POST', body: JSON.stringify({ force }) });
}

export function matchPolicy(prompt: string, files: string[] = []): Promise<MatchResult> {
  return request('/match', { method: 'POST', body: JSON.stringify({ prompt, files }) });
}

export function proposePolicyCapture(prompt: string, files: string[] = []): Promise<CaptureProposeResult> {
  return request('/capture', { method: 'POST', body: JSON.stringify({ action: 'propose', prompt, files }) });
}

export function savePolicyCapture(frontmatter: RuleFrontmatter, body: string): Promise<CaptureSaveResult> {
  return request('/capture', {
    method: 'POST',
    body: JSON.stringify({ action: 'save', prompt: '', rule: { frontmatter, body } }),
  });
}

export interface PolicyPackagePreviewItem {
  kind: string;
  id: string;
  status: string;
  compare?: string;
  newer?: string;
  summary?: string;
  reason?: string;
}

export interface PolicyPackageItemDiff {
  kind: string;
  id: string;
  compare: string;
  unified: string;
}

export interface PolicyPackagePreview {
  name: string;
  items: PolicyPackagePreviewItem[];
}

export interface PolicyPackageRestoreResult {
  written: string[];
  skipped: string[];
  errors: string[];
}

async function throwIfNotOk(res: Response): Promise<void> {
  if (res.ok) return;
  const body = (await res.json().catch(() => ({}))) as ApiErrorBody;
  throw new Error(formatApiError(body, res.status));
}

export async function downloadPolicyPackage(input: {
  name: string;
  description?: string;
  ruleIds: string[];
  skillNames: string[];
}): Promise<{ blob: Blob; filename: string }> {
  const res = await fetch(`${POLICY}/package`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  await throwIfNotOk(res);
  const cd = res.headers.get('content-disposition') ?? '';
  const match = /filename="([^"]+)"/.exec(cd);
  const filename = match?.[1] ?? 'package.ax-policy.zip';
  return { blob: await res.blob(), filename };
}

export async function previewPolicyPackage(file: File): Promise<PolicyPackagePreview> {
  const fd = new FormData();
  fd.append('package', file);
  const res = await fetch(`${POLICY}/package/preview`, { method: 'POST', body: fd });
  await throwIfNotOk(res);
  return res.json() as Promise<PolicyPackagePreview>;
}

export async function diffPolicyPackageItem(
  file: File,
  kind: string,
  id: string,
): Promise<PolicyPackageItemDiff> {
  const fd = new FormData();
  fd.append('package', file);
  fd.append('kind', kind);
  fd.append('id', id);
  const res = await fetch(`${POLICY}/package/diff`, { method: 'POST', body: fd });
  await throwIfNotOk(res);
  return res.json() as Promise<PolicyPackageItemDiff>;
}

export async function restorePolicyPackage(
  file: File,
  decisions: Record<string, 'overwrite' | 'skip'>,
): Promise<PolicyPackageRestoreResult> {
  const fd = new FormData();
  fd.append('package', file);
  fd.append('decisions', JSON.stringify(decisions));
  const res = await fetch(`${POLICY}/package/restore`, { method: 'POST', body: fd });
  await throwIfNotOk(res);
  return res.json() as Promise<PolicyPackageRestoreResult>;
}
