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

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${POLICY}${path}`, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
    ...init,
  });
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `HTTP ${res.status}`);
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
