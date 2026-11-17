import { useCallback, useEffect, useState } from 'react';
import {
  precisionText,
  promotedText,
  rankLatencyText,
  tokenDeltaText,
  type Metric,
  type PromotedModel,
} from './neuralMetrics';

type Capability = {
  status: string;
  kind?: string;
  detail?: string;
};

type NeuralStatus = {
  enabled: boolean;
  mode: string;
  backend: string;
  model: string;
  runtimeBuilt: boolean;
  mlRuntimeLinked: boolean;
  rankingActive?: boolean;
  /** Deprecated — always false until real neural embedding inference is active. */
  semanticActive?: boolean;
  gnn: string;
  configError: string | null;
  promotedModels?: PromotedModel[];
  capabilities?: {
    lexicalRetrieval?: Capability;
    hashVectorRetrieval?: Capability;
    neuralEmbeddingInference?: Capability;
    neuralRanking?: Capability;
    statisticalGraph?: Capability;
    graphNeuralNetwork?: Capability;
    prediction?: Capability;
    feedbackCollection?: Capability;
    learningAndTraining?: Capability;
    modelPromotion?: Capability;
    hardwareAcceleration?: Capability;
  };
  metrics?: {
    feedbackCount?: Metric;
    tokenDelta?: Metric;
    rankLatency?: Metric;
    predictionAccuracy?: Metric;
  };
};

function capLabel(cap: Capability | undefined, fallback: string): string {
  return cap?.status ?? fallback;
}

/** Settings → Embeddings: Neural is required and cannot be turned off. */
export default function NeuralSettingsSection() {
  const [status, setStatus] = useState<NeuralStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);

  const load = useCallback(() => {
    setTick((t) => t + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetch('/api/neural/status')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d: NeuralStatus) => {
        if (cancelled) return;
        setStatus(d);
        setErr(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setErr(e instanceof Error ? e.message : 'Failed to load Neural status');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tick]);

  const metrics = status?.metrics;
  const feedback = metrics?.feedbackCount;

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Neural</span>
          <span className="settings-row-desc">
            {err
              ? err
              : loading
                ? 'Checking Neural…'
                : 'Required. Memory embeddings stay hash-v1. Nothing is downloaded.'}
          </span>
        </div>
        <div className="settings-row-control settings-row-control--actions">
          <span className="settings-embed-badge settings-embed-badge--hash" title={status?.model ?? 'hash-v1'}>
            required
          </span>
          <button type="button" className="btn btn-subtle" disabled={loading} onClick={load}>
            {loading ? 'Checking…' : 'Refresh'}
          </button>
        </div>
      </div>
      <p className="settings-meta">
        Backend {status?.backend ?? 'hash'} · model {status?.model ?? 'hash-v1'} · runtime{' '}
        {status?.runtimeBuilt ? 'built' : 'not built'} · ML linked {status?.mlRuntimeLinked ? 'yes' : 'no'} · lexical{' '}
        {capLabel(status?.capabilities?.lexicalRetrieval, 'unknown')} · neural embed{' '}
        {capLabel(status?.capabilities?.neuralEmbeddingInference, 'not_built')} · ranking{' '}
        {capLabel(status?.capabilities?.neuralRanking, status?.rankingActive ? 'active' : 'disabled')} · GNN{' '}
        {capLabel(status?.capabilities?.graphNeuralNetwork, status?.gnn ?? 'not_built')}
      </p>
      <p className="settings-meta">
        Token delta {tokenDeltaText(metrics?.tokenDelta)} · rank latency {rankLatencyText(metrics?.rankLatency)} ·
        feedback rows {feedback?.kind === 'measured' ? feedback.value : 'unavailable'}
      </p>
      <p className="settings-meta">
        Prediction {precisionText(metrics?.predictionAccuracy)} · promoted models{' '}
        {promotedText(status?.promotedModels)}
      </p>
      {status?.configError ? <p className="settings-callout">{status.configError}</p> : null}
    </>
  );
}
