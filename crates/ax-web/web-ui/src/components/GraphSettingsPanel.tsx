import { useState, type ReactNode } from 'react';
import { DEFAULT_GRAPH_SETTINGS, type GraphSettings } from '../lib/graphSettings';

interface Props {
  settings: GraphSettings;
  onChange: (next: GraphSettings) => void;
  onAnimate: () => void;
  onClose: () => void;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <section className="graph-settings-section">
      <button type="button" className="graph-settings-heading" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className="graph-settings-caret">{open ? '▾' : '▸'}</span> {title}
      </button>
      {open && <div className="graph-settings-body">{children}</div>}
    </section>
  );
}

function Toggle({ label, value, onChange }: { label: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="graph-settings-row">
      <span>{label}</span>
      <input type="checkbox" role="switch" className="graph-settings-switch" checked={value} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

function Slider(props: { label: string; value: number; min: number; max: number; step: number; onChange: (v: number) => void }) {
  return (
    <label className="graph-settings-slider">
      <span>{props.label}</span>
      <input
        type="range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
    </label>
  );
}

export default function GraphSettingsPanel({ settings, onChange, onAnimate, onClose }: Props) {
  const set = <K extends keyof GraphSettings>(key: K, value: GraphSettings[K]) => onChange({ ...settings, [key]: value });
  const setGroup = (i: number, patch: Partial<GraphSettings['groups'][number]>) =>
    set('groups', settings.groups.map((g, j) => (j === i ? { ...g, ...patch } : g)));

  return (
    <div className="graph-settings" role="dialog" aria-label="Graph settings" onPointerDown={(e) => e.stopPropagation()} onWheel={(e) => e.stopPropagation()}>
      <div className="graph-settings-top">
        <strong>Filters</strong>
        <span className="graph-settings-actions">
          <button type="button" className="graph-settings-icon" title="Reset to defaults" onClick={() => onChange({ ...DEFAULT_GRAPH_SETTINGS })}>↺</button>
          <button type="button" className="graph-settings-icon" title="Close" aria-label="Close" onClick={onClose}>×</button>
        </span>
      </div>
      <Section title="Filters">
        <input
          type="search"
          className="graph-settings-search"
          placeholder="Search files…"
          value={settings.search}
          onChange={(e) => set('search', e.target.value)}
        />
        <Toggle label="Labels" value={settings.labels} onChange={(v) => set('labels', v)} />
        <Toggle label="Attachments" value={settings.attachments} onChange={(v) => set('attachments', v)} />
        <Toggle label="Existing files only" value={settings.existingOnly} onChange={(v) => set('existingOnly', v)} />
        <Toggle label="Orphans" value={settings.orphans} onChange={(v) => set('orphans', v)} />
      </Section>
      <Section title="Groups">
        {settings.groups.map((g, i) => (
          <div key={i} className="graph-settings-group">
            <input type="color" aria-label="Group color" value={g.color} onChange={(e) => setGroup(i, { color: e.target.value })} />
            <input type="text" placeholder="Query…" value={g.query} onChange={(e) => setGroup(i, { query: e.target.value })} />
            <button type="button" className="graph-settings-icon" aria-label="Remove group" onClick={() => set('groups', settings.groups.filter((_, j) => j !== i))}>×</button>
          </div>
        ))}
        <button type="button" className="btn-secondary graph-settings-wide" onClick={() => set('groups', [...settings.groups, { query: '', color: '#c586c0' }])}>
          New group
        </button>
      </Section>
      <Section title="Display">
        <Toggle label="Arrows" value={settings.arrows} onChange={(v) => set('arrows', v)} />
        <Slider label="Text fade threshold" min={-3} max={3} step={0.1} value={settings.textFadeThreshold} onChange={(v) => set('textFadeThreshold', v)} />
        <Slider label="Node size" min={0.1} max={5} step={0.1} value={settings.nodeSize} onChange={(v) => set('nodeSize', v)} />
        <Slider label="Link thickness" min={0.1} max={5} step={0.1} value={settings.linkThickness} onChange={(v) => set('linkThickness', v)} />
        <button type="button" className="btn-secondary graph-settings-animate" onClick={onAnimate}>Animate</button>
      </Section>
      <Section title="Forces">
        <Slider label="Center force" min={0} max={1} step={0.01} value={settings.centerForce} onChange={(v) => set('centerForce', v)} />
        <Slider label="Repel force" min={0} max={20} step={0.5} value={settings.repelForce} onChange={(v) => set('repelForce', v)} />
        <Slider label="Link force" min={0} max={1} step={0.01} value={settings.linkForce} onChange={(v) => set('linkForce', v)} />
        <Slider label="Link distance" min={30} max={500} step={10} value={settings.linkDistance} onChange={(v) => set('linkDistance', v)} />
      </Section>
    </div>
  );
}
