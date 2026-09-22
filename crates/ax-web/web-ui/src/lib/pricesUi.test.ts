import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  CHART_INPUT,
  CHART_OUTPUT,
  CURATED_CURSOR_MODELS,
  chartDomain,
  chartX,
  formatContextLength,
  mergeCatalog,
  allZeroPrices,
  pickDefaultModelId,
  providerLabel,
  providerSlug,
} from './pricesUi.ts';
import type { PricingCatalogRow } from '../api.ts';

function row(partial: Partial<PricingCatalogRow> & Pick<PricingCatalogRow, 'model_id'>): PricingCatalogRow {
  return {
    date: '2026-09-16',
    source: 'openrouter',
    display_name: partial.model_id,
    provider: null,
    input_per_mtok: 1,
    output_per_mtok: 2,
    cache_read_per_mtok: null,
    blended_3_to_1: null,
    context_length: null,
    intelligence: null,
    coding: null,
    agentic: null,
    ...partial,
  };
}

describe('prices UI', () => {
  it('P3 context uses en-US grouping not a decimal point', () => {
    assert.equal(formatContextLength(131072), '131,072');
    assert.equal(formatContextLength(32768), '32,768');
    assert.equal(formatContextLength(null), '—');
  });

  it('P5 merge adds Cursor when OpenRouter omits it', () => {
    const merged = mergeCatalog([row({ model_id: 'amazon/nova-micro-v1', provider: 'amazon' })]);
    assert.ok(merged.some((m) => m.provider === 'cursor'));
    assert.ok(CURATED_CURSOR_MODELS.length >= 2);
    assert.ok(merged.some((m) => m.model_id === 'cursor/composer-2.5'));
  });

  it('P5 merge does not duplicate Cursor if already present', () => {
    const existing = row({
      model_id: 'cursor/composer-2.5',
      provider: 'cursor',
      display_name: 'from-or',
    });
    const merged = mergeCatalog([existing]);
    assert.equal(merged.filter((m) => m.model_id === 'cursor/composer-2.5').length, 1);
    assert.equal(merged.find((m) => m.model_id === 'cursor/composer-2.5')?.display_name, 'from-or');
  });

  it('P7 provider labels', () => {
    assert.equal(providerLabel('x-ai'), 'xAI');
    assert.equal(providerLabel('mistralai'), 'Mistral');
    assert.equal(providerLabel('aion-labs'), 'Aion Labs');
    assert.equal(providerLabel('openai'), 'OpenAI');
    assert.equal(providerSlug(row({ model_id: 'openai/gpt-4o', provider: 'openai' })), 'openai');
  });

  it('P1 chart colors stay distinct', () => {
    assert.notEqual(CHART_INPUT.toLowerCase(), CHART_OUTPUT.toLowerCase());
    assert.match(CHART_INPUT, /^#5eb8ff$/i);
    assert.match(CHART_OUTPUT, /^#e0a030$/i);
  });

  it('P2 single-point x is centered without divide-by-zero', () => {
    assert.equal(chartX(0, 1, 52, 572), 52 + 572 / 2);
    assert.ok(Number.isFinite(chartX(0, 1, 52, 572)));
  });

  it('chart domain pads and stays non-negative', () => {
    const d = chartDomain([0.035, 0.14]);
    assert.ok(d.min >= 0);
    assert.ok(d.max > 0.14);
    assert.ok(d.max > d.min);
  });

  it('C2 chart domain for all zeros is 0,0 not 0.01', () => {
    assert.deepEqual(chartDomain([0, 0, 0]), { min: 0, max: 0 });
  });


  it('C1 default model skips free $0 rows', () => {
    const models = [
      row({ model_id: 'cohere/north-mini-code:free', input_per_mtok: 0, output_per_mtok: 0 }),
      row({ model_id: 'amazon/nova-micro-v1', provider: 'amazon', input_per_mtok: 0.035, output_per_mtok: 0.14 }),
    ];
    assert.equal(pickDefaultModelId(models), 'amazon/nova-micro-v1');
  });

  it('C2 all-zero series is flagged so the chart does not fake a $0.01 axis', () => {
    assert.equal(allZeroPrices([0, 0, 0]), true);
    assert.equal(allZeroPrices([0, 0.14]), false);
  });
});
