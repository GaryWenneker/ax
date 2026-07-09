import { useEffect, useState } from 'react';
import { deletePolicyRule, fetchPolicyRules, proposePolicyCapture, savePolicyCapture } from '../policyApi';
import { usePageContext } from '../context/UiContext';
import type { CaptureProposal, PolicyRuleRow } from '../policyTypes';

interface Props {
  onEdit: (id: string | null) => void;
  onMatch: () => void;
}

export default function PolicyRulesPage({ onEdit, onMatch }: Props) {
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [capturePrompt, setCapturePrompt] = useState('');
  const [captureProposal, setCaptureProposal] = useState<CaptureProposal | null>(null);
  const [captureError, setCaptureError] = useState('');
  const [captureLoading, setCaptureLoading] = useState(false);

  useEffect(() => {
    fetchPolicyRules()
      .then((r) => setRules(r.rules))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  usePageContext('Rules', !loading && !error ? `${rules.length} rules` : undefined);

  async function remove(id: string) {
    if (!confirm(`Delete rule "${id}"?`)) return;
    await deletePolicyRule(id);
    setRules((prev) => prev.filter((r) => r.id !== id));
  }

  function openCapture() {
    setCaptureOpen(true);
    setCaptureProposal(null);
    setCaptureError('');
  }

  function closeCapture() {
    setCaptureOpen(false);
    setCapturePrompt('');
    setCaptureProposal(null);
    setCaptureError('');
  }

  async function runCapturePropose() {
    setCaptureLoading(true);
    setCaptureError('');
    setCaptureProposal(null);
    try {
      const result = await proposePolicyCapture(capturePrompt);
      if (!result.detected || !result.proposal) {
        setCaptureError('No directive detected. Try phrases like "je moet", "always", or prefix with @rule.');
        return;
      }
      setCaptureProposal(result.proposal);
    } catch (e) {
      setCaptureError(e instanceof Error ? e.message : 'Capture failed');
    } finally {
      setCaptureLoading(false);
    }
  }

  async function runCaptureSave() {
    if (!captureProposal) return;
    setCaptureLoading(true);
    setCaptureError('');
    try {
      const saved = await savePolicyCapture(captureProposal.frontmatter, captureProposal.body);
      const rulesRes = await fetchPolicyRules();
      setRules(rulesRes.rules);
      closeCapture();
      onEdit(saved.id);
    } catch (e) {
      setCaptureError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setCaptureLoading(false);
    }
  }

  if (loading) return <p className="muted">Loading rules…</p>;
  if (error) return <p className="error">{error}</p>;

  return (
    <div className="page">
      <div className="page-header">
        <h1>Rules</h1>
        <div className="page-actions">
          <button type="button" className="btn" onClick={onMatch}>Test match</button>
          <button type="button" className="btn" onClick={openCapture}>Capture from prompt</button>
          <button type="button" className="btn primary" onClick={() => onEdit(null)}>New rule</button>
        </div>
      </div>

      {captureOpen && (
        <section className="capture-panel">
          <div className="capture-panel-header">
            <h2>Capture from prompt</h2>
            <button type="button" className="btn" onClick={closeCapture}>Close</button>
          </div>
          <p className="muted capture-hint">
            Paste a durable instruction (e.g. &quot;je moet altijd Tailwind gebruiken&quot;). Preview first, then save.
          </p>
          <label className="full-width">
            Prompt
            <textarea
              className="match-prompt"
              value={capturePrompt}
              onChange={(e) => setCapturePrompt(e.target.value)}
              rows={4}
              placeholder="je moet altijd dark mode gebruiken"
            />
          </label>
          <div className="capture-actions">
            <button
              type="button"
              className="btn primary"
              disabled={captureLoading || !capturePrompt.trim()}
              onClick={runCapturePropose}
            >
              {captureLoading && !captureProposal ? 'Analyzing…' : 'Preview rule'}
            </button>
            {captureProposal && (
              <button
                type="button"
                className="btn primary"
                disabled={captureLoading}
                onClick={runCaptureSave}
              >
                {captureLoading ? 'Saving…' : 'Save rule'}
              </button>
            )}
          </div>
          {captureError && <p className="error">{captureError}</p>}
          {captureProposal && (
            <div className="capture-preview">
              <div className="capture-meta muted">
                <span>id: {captureProposal.suggestedId}</span>
                <span>confidence: {captureProposal.confidence}</span>
                <span>path: {captureProposal.previewPath}</span>
              </div>
              {(captureProposal.questions?.length ?? 0) > 0 && (
                <ul className="capture-questions">
                  {captureProposal.questions.map((q) => (
                    <li key={q.field}>
                      <strong>{q.field}</strong>: {q.question}
                      <span className="muted"> (current: {q.current})</span>
                    </li>
                  ))}
                </ul>
              )}
              <pre className="match-result">{captureProposal.preview}</pre>
            </div>
          )}
        </section>
      )}

      {rules.length === 0 ? (
        <p className="muted">No rules yet. Create your first rule.</p>
      ) : (
        <ul className="policy-list">
          {rules.map((r) => (
            <li key={r.id} className={`policy-item level-${r.level.toLowerCase()}`}>
              <div className="policy-item-main">
                <strong>{r.id}</strong>
                <span className="badge">{r.level}</span>
                <span className="muted">p{r.priority}</span>
              </div>
              <div className="policy-item-meta muted">
                {r.alwaysApply ? 'always · ' : ''}
                {r.globs.length} globs · {r.triggers.length} triggers
              </div>
              <div className="policy-item-actions">
                <button type="button" className="btn" onClick={() => onEdit(r.id)}>Edit</button>
                <button type="button" className="btn danger" onClick={() => remove(r.id)}>Delete</button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
