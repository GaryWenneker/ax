import type {
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

export function fetchPolicyRules(): Promise<{ rules: PolicyRuleRow[] }> {
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

export function fetchPolicySkills(): Promise<{ skills: PolicySkillRow[] }> {
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

export function matchPolicy(prompt: string, files: string[] = []): Promise<MatchResult> {
  return request('/match', { method: 'POST', body: JSON.stringify({ prompt, files }) });
}
