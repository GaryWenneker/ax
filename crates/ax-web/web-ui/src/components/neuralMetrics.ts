export type Metric = {
  value: number | null;
  kind: string;
  unit?: string;
  preflights?: number;
  source?: string;
  predictions?: number;
};

export type PromotedModel = { modelId: string; version: string; checksum: string };

function measured(metric?: Metric): metric is Metric & { value: number } {
  return metric?.kind === 'measured' && typeof metric.value === 'number';
}

/** Tokens removed from preflight replies by dropping duplicate blocks. */
export function tokenDeltaText(metric?: Metric): string {
  if (!measured(metric)) return 'unavailable';
  return `${metric.value} tokens over ${metric.preflights ?? 0} preflights`;
}

export function rankLatencyText(metric?: Metric): string {
  if (!measured(metric)) return 'unavailable';
  return `${Math.round(metric.value)} µs per preflight over ${metric.preflights ?? 0} preflights`;
}

export function precisionText(metric?: Metric): string {
  if (metric?.kind !== 'measured') return 'not evaluated; run ax neural evaluate';
  const source = metric.source ?? 'unknown source';
  if (typeof metric.value !== 'number') return `no predictions to score (${source})`;
  return `${Math.round(metric.value * 100)}% precision on ${metric.predictions ?? 0} predictions (${source})`;
}

export function promotedText(models?: PromotedModel[]): string {
  if (!models || models.length === 0) return 'none; memory stays hash-v1';
  return models
    .map((model) => {
      const checksum = model.checksum.length > 12 ? `${model.checksum.slice(0, 12)}…` : model.checksum;
      return `${model.modelId} ${model.version} (${checksum})`;
    })
    .join(', ');
}
