import { useState } from 'react';
import { matchPolicy } from '../policyApi';
import {
  PageCard,
  PageCardBody,
  PageHero,
  PageRow,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
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
    <PageShell>
      <PageHero
        title="Test match"
        subtitle="Preview which rules and skills would inject for a given prompt."
        actions={
          <button type="button" className="btn" onClick={onClose}>Close</button>
        }
      />

      <PageToasts err={error || null} />

      <PageStack>
        <PageCard
          title="Match simulator"
          description="Enter a prompt to see the policy inject output."
          footer={
            <button
              type="button"
              className="btn primary"
              disabled={loading || !prompt.trim()}
              onClick={run}
            >
              {loading ? 'Matching…' : 'Run match'}
            </button>
          }
        >
          <PageCardBody>
            <PageRow title="Prompt" description="Simulate an agent turn with this user message.">
              <textarea
                className="settings-input"
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                rows={4}
                style={{ resize: 'vertical', minHeight: 88 }}
              />
            </PageRow>
            {result && (
              <>
                <div className="settings-divider" />
                <div className="settings-subsection-label">Inject output</div>
                <pre className="page-code-block">{result}</pre>
              </>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
