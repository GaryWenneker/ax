import { useState } from 'react';
import { matchPolicy } from '../policyApi';
import { usePageContext } from '../context/UiContext';

interface Props {
  onClose: () => void;
}

export default function PolicyMatchPage({ onClose }: Props) {
  const [prompt, setPrompt] = useState('');
  const [result, setResult] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  usePageContext('Match test', prompt.trim() ? 'prompt ready' : undefined);

  async function run() {
    setLoading(true);
    setError('');
    try {
      const r = await matchPolicy(prompt);
      setResult(r.inject || JSON.stringify(r, null, 2));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Match failed');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>Test match</h1>
        <button type="button" className="btn" onClick={onClose}>Close</button>
      </div>
      <label className="full-width">
        Prompt
        <textarea className="match-prompt" value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={4} />
      </label>
      <button type="button" className="btn primary" disabled={loading || !prompt.trim()} onClick={run}>
        {loading ? 'Matching…' : 'Run match'}
      </button>
      {error && <p className="error">{error}</p>}
      {result && <pre className="match-result">{result}</pre>}
    </div>
  );
}
