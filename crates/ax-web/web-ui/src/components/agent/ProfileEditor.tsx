import { useEffect, useState } from 'react';

import { updateAgentProfile, type ProfileEntry } from '../../agentApi';

interface Props {
  agent: string;
  profile: ProfileEntry;
  readonly: boolean;
  onSaved: () => void;
  onError: (msg: string) => void;
}

export default function ProfileEditor({ agent, profile, readonly, onSaved, onError }: Props) {
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState(profile.label);
  const [provider, setProvider] = useState(profile.provider ?? '');
  const [keyEnv, setKeyEnv] = useState(profile.key_env ?? '');
  const [model, setModel] = useState(profile.model ?? '');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setLabel(profile.label);
    setProvider(profile.provider ?? '');
    setKeyEnv(profile.key_env ?? '');
    setModel(profile.model ?? '');
  }, [profile]);

  async function save() {
    if (readonly) {
      onError('Read-only mode — changes cannot be saved');
      return;
    }
    setSaving(true);
    const res = await updateAgentProfile(agent, profile.id, {
      label: label.trim(),
      provider: provider.trim() || undefined,
      key_env: keyEnv.trim() || undefined,
      model: model.trim() || undefined,
    });
    setSaving(false);
    if (!res.ok) {
      onError(res.error ?? 'Failed to save profile');
      return;
    }
    setOpen(false);
    onSaved();
  }

  const authLabel =
    profile.auth_status === 'authenticated'
      ? 'Authenticated'
      : profile.auth_status === 'needs_auth'
        ? 'Needs auth'
        : 'Unknown';

  const authTone =
    profile.auth_status === 'authenticated' ? 'ok' : profile.auth_status === 'needs_auth' ? 'warn' : 'muted';

  return (
    <div className="agent-profile-row">
      <div className="agent-profile-row-main">
        <span className="agent-profile-label">{profile.label}</span>
        <span className="agent-profile-id">{profile.id}</span>
        {agent !== 'builtin' && (
          <span className={`agent-profile-pill agent-profile-pill--${authTone}`}>{authLabel}</span>
        )}
        {agent === 'builtin' && (profile.provider || profile.model) && (
          <span className="agent-profile-meta">
            {[profile.provider, profile.model].filter(Boolean).join(' · ')}
          </span>
        )}
      </div>
      <div className="agent-profile-row-actions">
        <button type="button" className="btn btn-subtle btn-sm" onClick={() => setOpen((v) => !v)}>
          {open ? 'Close' : 'Edit'}
        </button>
      </div>
      {open && (
        <div className="agent-profile-form">
          <label className="agent-profile-field">
            <span>Label</span>
            <input className="settings-input" value={label} disabled={readonly} onChange={(e) => setLabel(e.target.value)} />
          </label>
          {agent === 'builtin' && (
            <>
              <label className="agent-profile-field">
                <span>Provider</span>
                <select className="settings-input" value={provider} disabled={readonly} onChange={(e) => setProvider(e.target.value)}>
                  <option value="">Default</option>
                  <option value="openai">OpenAI</option>
                  <option value="anthropic">Anthropic</option>
                  <option value="google">Google</option>
                  <option value="openrouter">OpenRouter</option>
                </select>
              </label>
              <label className="agent-profile-field">
                <span>API key env var</span>
                <input className="settings-input" placeholder="OPENAI_API_KEY" value={keyEnv} disabled={readonly} onChange={(e) => setKeyEnv(e.target.value)} />
              </label>
              <label className="agent-profile-field">
                <span>Model</span>
                <input className="settings-input" placeholder="gpt-4o-mini" value={model} disabled={readonly} onChange={(e) => setModel(e.target.value)} />
              </label>
            </>
          )}
          <button type="button" className="btn primary btn-sm" disabled={saving || readonly || !label.trim()} onClick={() => void save()}>
            {saving ? 'Saving…' : 'Save profile'}
          </button>
        </div>
      )}
    </div>
  );
}
