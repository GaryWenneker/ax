import type { PricingCatalogRow } from '../api';

/** Chart series colors — independent of theme accent (Mono would otherwise hide both lines). */
export const CHART_INPUT = '#5eb8ff';
export const CHART_OUTPUT = '#e0a030';

export const PROVIDER_LABELS: Record<string, string> = {
  'aion-labs': 'Aion Labs',
  'anthracite-org': 'Anthracite',
  'arcee-ai': 'Arcee',
  'bytedance-seed': 'ByteDance Seed',
  'cognitivecomputations': 'Cognitive Computations',
  'ibm-granite': 'IBM Granite',
  'meta-llama': 'Meta Llama',
  'mistralai': 'Mistral',
  'moonshotai': 'Moonshot',
  'nousresearch': 'Nous Research',
  openai: 'OpenAI',
  openrouter: 'OpenRouter',
  'rekaai': 'Reka',
  'thedrummer': 'The Drummer',
  'thinkingmachines': 'Thinking Machines',
  'x-ai': 'xAI',
  'z-ai': 'Z.AI',
  '~anthropic': 'Anthropic (auto)',
  '~deepseek': 'DeepSeek (auto)',
  '~google': 'Google (auto)',
  '~moonshotai': 'Moonshot (auto)',
  '~openai': 'OpenAI (auto)',
  '~x-ai': 'xAI (auto)',
  '~z-ai': 'Z.AI (auto)',
};

/** Built-in Cursor rates from Savings defaults — OpenRouter does not list Cursor. */
export const CURATED_CURSOR_MODELS: PricingCatalogRow[] = [
  {
    date: '',
    source: 'builtin',
    model_id: 'cursor/composer-2.5',
    display_name: 'Composer 2.5',
    provider: 'cursor',
    input_per_mtok: 1.25,
    output_per_mtok: 10,
    cache_read_per_mtok: null,
    blended_3_to_1: null,
    context_length: 200000,
    intelligence: null,
    coding: null,
    agentic: null,
  },
  {
    date: '',
    source: 'builtin',
    model_id: 'cursor/composer-2.5-fast',
    display_name: 'Composer 2.5 Fast',
    provider: 'cursor',
    input_per_mtok: 1.25,
    output_per_mtok: 10,
    cache_read_per_mtok: null,
    blended_3_to_1: null,
    context_length: 200000,
    intelligence: null,
    coding: null,
    agentic: null,
  },
];

export function providerLabel(slug: string): string {
  const key = slug.trim().toLowerCase();
  if (PROVIDER_LABELS[key]) return PROVIDER_LABELS[key];
  if (!key) return 'Other';
  return key
    .split(/[-_]/)
    .map((p) => (p.startsWith('~') ? p : p.charAt(0).toUpperCase() + p.slice(1)))
    .join(' ');
}

export function providerSlug(row: PricingCatalogRow): string {
  const raw = (row.provider || row.model_id.split('/')[0] || 'other').toLowerCase();
  return raw || 'other';
}

export function formatContextLength(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n) || n <= 0) return '—';
  return Math.round(n).toLocaleString('en-US');
}

export function mergeCatalog(openrouter: PricingCatalogRow[]): PricingCatalogRow[] {
  const ids = new Set(openrouter.map((r) => r.model_id.toLowerCase()));
  const extra = CURATED_CURSOR_MODELS.filter((r) => !ids.has(r.model_id.toLowerCase()));
  return extra.length ? [...openrouter, ...extra] : openrouter;
}

export function chartX(i: number, count: number, padL: number, innerW: number): number {
  if (count <= 1) return padL + innerW / 2;
  return padL + (i / (count - 1)) * innerW;
}

export function chartY(v: number, min: number, max: number, padT: number, innerH: number): number {
  const span = max - min;
  if (!(span > 0)) return padT + innerH / 2;
  return padT + ((max - v) / span) * innerH;
}

export function allZeroPrices(values: number[]): boolean {
  const finite = values.filter((n) => Number.isFinite(n));
  if (!finite.length) return true;
  return finite.every((n) => n === 0);
}

export function pickDefaultModelId(models: PricingCatalogRow[]): string | null {
  const paid = models.find(
    (m) =>
      !m.model_id.toLowerCase().includes(':free') &&
      ((m.input_per_mtok ?? 0) > 0 || (m.output_per_mtok ?? 0) > 0),
  );
  if (paid) return paid.model_id;
  return models[0]?.model_id ?? null;
}

export function chartDomain(values: number[]): { min: number; max: number } {
  const finite = values.filter((n) => Number.isFinite(n));
  if (!finite.length || allZeroPrices(finite)) return { min: 0, max: 0 };
  let min = Math.min(...finite);
  let max = Math.max(...finite);
  if (max === min) {
    min = 0;
    max = max * 1.15;
  } else {
    const pad = (max - min) * 0.08;
    min = Math.max(0, min - pad);
    max = max + pad;
  }
  return { min, max };
}
